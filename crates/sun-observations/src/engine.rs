use crate::{
    channel::{dispatch, flush_sinks},
    context,
    namespace::InvalidNamespace,
    plan::{next_identity, ResolvedPlan},
    routing::Routing,
    scope::ScopeState,
    span::SpanGuard,
    Observation, ObservationLevel, ObservationRecord, Phase, Policy, ScopeConfig, Selection, Sink,
};
use chrono::{SecondsFormat, Utc};
use parking_lot::RwLock;
use serde_json::Value;
use std::{
    any::TypeId,
    error::Error,
    fmt, io,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock,
    },
    time::Instant,
};
use uuid::Uuid;

#[derive(Debug)]
pub enum ObservationError {
    Serialize(serde_json::Error),
    Sink(io::Error),
}
impl fmt::Display for ObservationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serialize(e) => write!(f, "observation serialization: {e}"),
            Self::Sink(e) => write!(f, "observation sink: {e}"),
        }
    }
}
impl Error for ObservationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Serialize(e) => e,
            Self::Sink(e) => e,
        })
    }
}
#[derive(Default)]
struct Configuration {
    selection: Selection,
    routing: Routing,
}

pub struct ObservationEngine {
    pub(crate) id: u64,
    process: String,
    started: Instant,
    config: RwLock<Configuration>,
    generation: AtomicU64,
    pub(crate) unscoped: ScopeState,
    sink_errors: AtomicU64,
}
pub(crate) type Admission = Arc<ResolvedPlan>;
impl Default for ObservationEngine {
    fn default() -> Self {
        Self::new()
    }
}
impl ObservationEngine {
    pub fn new() -> Self {
        Self {
            id: next_identity(),
            process: format!("{}_{}", std::process::id(), Uuid::new_v4().simple()),
            started: Instant::now(),
            config: RwLock::new(Configuration::default()),
            generation: AtomicU64::new(1),
            unscoped: ScopeState::new(ScopeConfig::new("")),
            sink_errors: AtomicU64::new(0),
        }
    }
    pub fn process_id(&self) -> &str {
        &self.process
    }
    #[inline]
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    pub fn set_selection(&self, selection: Selection) {
        let mut config = self.config.write();
        config.selection = selection;
        self.generation.fetch_add(1, Ordering::Release);
    }
    /// Central attachment boundary; publishes a new generation for all cached plans.
    pub fn add_channel_sink(&self, channel: &str, sink: Arc<dyn Sink>) {
        let needs_render_text = sink.wants_render_text();
        let mut config = self.config.write();
        config.routing.add_sink(channel, sink, needs_render_text);
        self.generation.fetch_add(1, Ordering::Release);
    }
    /// Detach this channel's sinks; existing spans keep their start-time snapshots.
    pub fn clear_channel_sinks(&self, channel: &str) {
        let mut config = self.config.write();
        let retired = config.routing.clear_sinks(channel);
        self.generation.fetch_add(1, Ordering::Release);
        drop(config);
        drop(retired);
    }
    pub fn register_namespace_channel(
        &self,
        namespace: &str,
        channel: &str,
    ) -> Result<(), InvalidNamespace> {
        let mut config = self.config.write();
        config.routing.register(namespace, channel)?;
        self.generation.fetch_add(1, Ordering::Release);
        Ok(())
    }
    pub fn sink_error_count(&self) -> u64 {
        self.sink_errors.load(Ordering::Relaxed)
    }
    /// Wait for each unique attached sink's flush barrier. No engine locks held while waiting.
    pub fn flush_channels(&self) -> Result<(), ObservationError> {
        let sinks = self.config.read().routing.sinks();
        self.check_sink_result(flush_sinks(&sinks))
    }
    pub(crate) fn resolve<T: Observation>(&self, scope: &ScopeState) -> Arc<ResolvedPlan> {
        let mut cache = scope.plans.lock();
        let config = self.config.read();
        let generation = self.generation();
        let key = (self.id, TypeId::of::<T>());
        if let Some(plan) = cache.plans.get(&key) {
            if plan.generation == generation {
                return plan.clone();
            }
        }
        let global = &config.selection;
        let local = &scope.config.selection;
        let capture = scope
            .capture
            .as_ref()
            .filter(|capture| capture.engine == self.id);
        let active = if let Some(capture) = capture {
            capture.selection.admits(T::NAMESPACE, T::TAGS)
                && !local.excludes(T::NAMESPACE, T::TAGS)
        } else {
            (global.activates(T::NAMESPACE, T::TAGS) || local.activates(T::NAMESPACE, T::TAGS))
                && !global.excludes(T::NAMESPACE, T::TAGS)
                && !local.excludes(T::NAMESPACE, T::TAGS)
        };
        let policy = scope
            .config
            .policies
            .get(&TypeId::of::<T>())
            .copied()
            .unwrap_or(Policy::Always);
        let counter = if active && !matches!(policy, Policy::Always | Policy::Never) {
            Some(cache.counters.entry(TypeId::of::<T>()).or_default().clone())
        } else {
            None
        };
        let mut needs_render_text = false;
        let sinks: Vec<Arc<dyn Sink>> = if let Some(capture) = capture.filter(|_| active) {
            vec![capture.clone()]
        } else if active {
            config
                .routing
                .matching(T::NAMESPACE)
                .iter()
                .flat_map(|c| {
                    needs_render_text |= c.needs_render_text;
                    c.sinks.iter().cloned()
                })
                .collect()
        } else {
            Vec::new()
        };
        let plan = Arc::new(ResolvedPlan {
            generation,
            capture: capture.cloned(),
            active: active && policy != Policy::Never,
            policy,
            counter,
            scope: (scope.id != self.unscoped.id).then(|| scope.config.name.clone()),
            needs_render_text,
            sinks: sinks.into(),
        });
        let retired = cache.plans.insert(key, plan.clone());
        drop(config);
        drop(cache);
        drop(retired);
        plan
    }
    fn serialize<T: Observation>(
        plan: &ResolvedPlan,
        payload: &T,
    ) -> Result<Value, ObservationError> {
        serde_json::to_value(payload).map_err(|error| {
            if let Some(capture) = &plan.capture {
                capture.emission_failed();
            }
            ObservationError::Serialize(error)
        })
    }
    pub(crate) fn record(
        &self,
        plan: &ResolvedPlan,
        namespace: &str,
        level: ObservationLevel,
        data: Value,
    ) -> ObservationRecord {
        ObservationRecord {
            ts: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            proc: self.process.clone(),
            proc_ms: self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            scope: plan.scope.clone(),
            namespace: namespace.to_owned(),
            level,
            data,
            message: None,
            phase: None,
            span: None,
        }
    }
    fn check_sink_result(&self, result: io::Result<()>) -> Result<(), ObservationError> {
        result.map_err(|e| {
            self.sink_errors.fetch_add(1, Ordering::Relaxed);
            ObservationError::Sink(e)
        })
    }
    pub(crate) fn dispatch(
        &self,
        plan: &ResolvedPlan,
        record: ObservationRecord,
        render_text: Option<String>,
    ) -> Result<(), ObservationError> {
        let record = Arc::new(record);
        let result = if plan.needs_render_text {
            let delivery = crate::ObservationDelivery::new(record, render_text.map(Arc::new));
            crate::channel::dispatch_delivery(&plan.sinks, &delivery)
        } else {
            dispatch(&plan.sinks, record)
        };
        self.check_sink_result(result)
    }
    #[inline]
    pub fn observe_if_active<T: Observation, F: FnOnce() -> T>(
        &self,
        build: F,
    ) -> Result<bool, ObservationError> {
        let plan = context::cached_plan::<T>(self);
        if !plan.admit() {
            return Ok(false);
        }
        let payload = build();
        let data = Self::serialize(&plan, &payload)?;
        let message = payload.message();
        // No consumer can observe envelope allocation/formatting, but payload serialization
        // and message callbacks still run (including their errors/side effects).
        if plan.sinks.is_empty() {
            return Ok(true);
        }
        let mut record = self.record(&plan, T::NAMESPACE, T::LEVEL, data);
        record.message = message.map(str::to_owned);
        let render_text = if plan.needs_render_text {
            payload.render_text()
        } else {
            None
        };
        self.dispatch(&plan, record, render_text)?;
        Ok(true)
    }
    pub fn span_if_active<T: Observation, F: FnOnce() -> T>(
        &self,
        build: F,
    ) -> Result<SpanGuard<'_>, ObservationError> {
        let plan = context::cached_plan::<T>(self);
        if !plan.admit() {
            return Ok(SpanGuard::inactive(self));
        }
        let payload = build();
        let data = Self::serialize(&plan, &payload)?;
        let mut record = self.record(&plan, T::NAMESPACE, T::LEVEL, data);
        record.message = payload.message().map(str::to_owned);
        record.phase = Some(Phase::Start);
        let span = Uuid::new_v4().simple().to_string();
        record.span = Some(span.clone());
        self.dispatch(&plan, record, None)?;
        Ok(SpanGuard::active(
            self,
            (*plan).clone(),
            T::NAMESPACE,
            T::LEVEL,
            span,
        ))
    }
}
impl Drop for ObservationEngine {
    fn drop(&mut self) {
        if let Err(error) = self.flush_channels() {
            eprintln!("SunObservations shutdown: {error}");
        }
        context::forget_engine(self.id);
    }
}
pub fn global() -> &'static ObservationEngine {
    static ENGINE: OnceLock<ObservationEngine> = OnceLock::new();
    ENGINE.get_or_init(ObservationEngine::new)
}
#[inline]
pub fn observe_if_active<T: Observation, F: FnOnce() -> T>(
    build: F,
) -> Result<bool, ObservationError> {
    global().observe_if_active(build)
}

#[cfg(test)]
#[path = "engine_test.rs"]
mod tests;

#[cfg(test)]
#[path = "engine_certification_test.rs"]
mod certification;

#[cfg(test)]
#[path = "render_certification_test.rs"]
mod human_certification;
