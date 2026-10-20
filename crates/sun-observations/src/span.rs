use crate::{
    engine::Admission, Observation, ObservationEngine, ObservationError, ObservationLevel, Phase,
};

struct ActiveSpan {
    admission: Admission,
    namespace: &'static str,
    level: ObservationLevel,
    id: String,
}

/// Admission and routing are frozen at start; end survives scope/configuration changes.
/// Drop cannot return I/O errors: use finish() when they must be observed synchronously.
#[must_use = "dropping the guard immediately ends the span"]
pub struct SpanGuard<'a> {
    engine: &'a ObservationEngine,
    active: Option<ActiveSpan>,
}
impl<'a> SpanGuard<'a> {
    pub(crate) fn inactive(engine: &'a ObservationEngine) -> Self {
        Self {
            engine,
            active: None,
        }
    }
    pub(crate) fn active(
        engine: &'a ObservationEngine,
        admission: Admission,
        namespace: &'static str,
        level: ObservationLevel,
        id: String,
    ) -> Self {
        Self {
            engine,
            active: Some(ActiveSpan {
                admission,
                namespace,
                level,
                id,
            }),
        }
    }
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }
    pub fn finish(mut self) -> Result<(), ObservationError> {
        self.end()
    }
    fn end(&mut self) -> Result<(), ObservationError> {
        if let Some(span) = self.active.take() {
            let mut record = self.engine.record(
                &span.admission,
                span.namespace,
                span.level,
                serde_json::Value::Null,
            );
            record.phase = Some(Phase::End);
            record.span = Some(span.id);
            self.engine.dispatch(&span.admission, record)?;
        }
        Ok(())
    }
}
impl Drop for SpanGuard<'_> {
    fn drop(&mut self) {
        let _ = self.end();
    }
}

pub fn span_if_active<T: Observation, F: FnOnce() -> T>(
    build: F,
) -> Result<SpanGuard<'static>, ObservationError> {
    crate::global().span_if_active(build)
}

#[cfg(test)]
#[path = "span_test.rs"]
mod tests;
