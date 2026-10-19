use crate::{
    channel::{dispatch, flush_sinks},
    context,
    namespace::InvalidNamespace,
    routing::Routing,
    scope::ScopeState,
    span::SpanGuard,
    Channel, Observation, ObservationLevel, ObservationRecord, Phase, ScopeConfig, Selection, Sink,
};
use chrono::{SecondsFormat, Utc};
use parking_lot::RwLock;
use serde_json::Value;
use std::{
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

/// Process-global entry point; independent engines are also useful for isolated embedding.
pub struct ObservationEngine {
    process: String,
    started: Instant,
    selection: RwLock<Selection>,
    routing: RwLock<Routing>,
    unscoped: ScopeState,
    sink_errors: AtomicU64,
}

pub(crate) struct Admission {
    pub scope: Option<String>,
    pub sinks: Vec<Arc<dyn Sink>>,
}

impl Default for ObservationEngine {
    fn default() -> Self {
        Self::new()
    }
}
impl ObservationEngine {
    pub fn new() -> Self {
        Self {
            process: format!("{}_{}", std::process::id(), Uuid::new_v4().simple()),
            started: Instant::now(),
            selection: RwLock::new(Selection::new()),
            routing: RwLock::new(Routing::default()),
            unscoped: ScopeState::new(ScopeConfig::new("")),
            sink_errors: AtomicU64::new(0),
        }
    }
    pub fn process_id(&self) -> &str {
        &self.process
    }
    /// Atomically replace process-wide selectors; exclusions still veto scope activations.
    pub fn set_selection(&self, selection: Selection) {
        *self.selection.write() = selection;
    }
    pub fn channel(&self, name: &str) -> Arc<Channel> {
        self.routing.write().channel(name)
    }
    pub fn register_namespace_channel(
        &self,
        namespace: &str,
        channel: &str,
    ) -> Result<(), InvalidNamespace> {
        self.routing.write().register(namespace, channel)
    }
    /// Number of failed dispatch/flush operations, including errors during span Drop.
    pub fn sink_error_count(&self) -> u64 {
        self.sink_errors.load(Ordering::Relaxed)
    }
    pub fn flush(&self) -> Result<(), ObservationError> {
        let channels = self.routing.read().all();
        let sinks: Vec<_> = channels.iter().flat_map(|c| c.sinks()).collect();
        self.check_sink_result(flush_sinks(&sinks))
    }
    pub(crate) fn admit<T: Observation>(&self) -> Option<Admission> {
        let scope = context::current_scope();
        let selection = self.selection.read();
        let local = scope.as_ref().map(|s| &s.config.selection);
        let excluded = selection.excludes(T::NAMESPACE, T::TAGS)
            || local.is_some_and(|s| s.excludes(T::NAMESPACE, T::TAGS));
        let active = selection.activates(T::NAMESPACE, T::TAGS)
            || local.is_some_and(|s| s.activates(T::NAMESPACE, T::TAGS));
        drop(selection);
        if excluded || !active {
            return None;
        }
        if !scope.as_deref().unwrap_or(&self.unscoped).admit::<T>() {
            return None;
        }
        let channels = self.routing.read().matching(T::NAMESPACE);
        Some(Admission {
            scope: scope.as_ref().map(|s| s.config.name.clone()),
            sinks: channels.iter().flat_map(|c| c.sinks()).collect(),
        })
    }
    pub(crate) fn record(
        &self,
        admission: &Admission,
        namespace: &str,
        level: ObservationLevel,
        data: Value,
    ) -> ObservationRecord {
        ObservationRecord {
            ts: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            proc: self.process.clone(),
            proc_ms: self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            scope: admission.scope.clone(),
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
        admission: &Admission,
        record: &ObservationRecord,
    ) -> Result<(), ObservationError> {
        self.check_sink_result(dispatch(&admission.sinks, record))
    }
    /// Admission precedes payload construction and serialization. Rejection is Ok(false).
    pub fn observe_if_active<T: Observation, F: FnOnce() -> T>(
        &self,
        build: F,
    ) -> Result<bool, ObservationError> {
        let Some(admission) = self.admit::<T>() else {
            return Ok(false);
        };
        let payload = build();
        let data = serde_json::to_value(&payload).map_err(ObservationError::Serialize)?;
        let mut record = self.record(&admission, T::NAMESPACE, T::LEVEL, data);
        record.message = payload.message().map(str::to_owned);
        self.dispatch(&admission, &record)?;
        Ok(true)
    }
    pub fn span_if_active<T: Observation, F: FnOnce() -> T>(
        &self,
        build: F,
    ) -> Result<SpanGuard<'_>, ObservationError> {
        let Some(admission) = self.admit::<T>() else {
            return Ok(SpanGuard::inactive(self));
        };
        let payload = build();
        let data = serde_json::to_value(&payload).map_err(ObservationError::Serialize)?;
        let mut record = self.record(&admission, T::NAMESPACE, T::LEVEL, data);
        record.message = payload.message().map(str::to_owned);
        record.phase = Some(Phase::Start);
        let span = Uuid::new_v4().simple().to_string();
        record.span = Some(span.clone());
        self.dispatch(&admission, &record)?;
        Ok(SpanGuard::active(
            self,
            admission,
            T::NAMESPACE,
            T::LEVEL,
            span,
        ))
    }
}

pub fn global() -> &'static ObservationEngine {
    static ENGINE: OnceLock<ObservationEngine> = OnceLock::new();
    ENGINE.get_or_init(ObservationEngine::new)
}
pub fn observe_if_active<T: Observation, F: FnOnce() -> T>(
    build: F,
) -> Result<bool, ObservationError> {
    global().observe_if_active(build)
}

#[cfg(test)]
#[path = "engine_test.rs"]
mod tests;
