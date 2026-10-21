//! Runtime context attachment used by the Observation data-source capture adapter.
use crate::{
    context, scope::ScopeState, ContextGuard, MemorySink, MemorySinkConfig, MemoryStats,
    ObservationRecord, ScopeConfig, Selection, Sink,
};
use std::{
    io,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

pub(crate) struct CaptureStorage {
    pub engine: u64,
    pub selection: Selection,
    memory: MemorySink,
    errors: AtomicU64,
}
impl CaptureStorage {
    pub(crate) fn emission_failed(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }
}
impl Sink for CaptureStorage {
    fn write(&self, record: &ObservationRecord) -> io::Result<()> {
        self.memory.write(record)
    }
    fn write_shared(&self, record: Arc<ObservationRecord>) -> io::Result<()> {
        self.memory.write_shared(record)
    }
}
/// Completeness at the synchronous execution boundary, independent of semantic acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureStatus {
    pub retention: MemoryStats,
    pub emission_errors: u64,
    pub unfinished_contexts: bool,
}
impl CaptureStatus {
    pub fn is_complete(&self) -> bool {
        self.retention.evicted == 0 && self.emission_errors == 0 && !self.unfinished_contexts
    }
}
/// RAII runtime attachment. Nested captures are exclusive; propagated contexts must finish before finalization.
#[must_use]
pub struct ScopedCapture {
    guard: Option<ContextGuard>,
    storage: Arc<CaptureStorage>,
}
impl ScopedCapture {
    pub fn new(selection: Selection, config: MemorySinkConfig) -> Self {
        let storage = Arc::new(CaptureStorage {
            engine: crate::global().id,
            selection,
            memory: MemorySink::with_config(config),
            errors: AtomicU64::new(0),
        });
        let mut scope = ScopeState::new(ScopeConfig::new("capture"));
        scope.capture = Some(storage.clone());
        Self {
            guard: Some(context::enter_scope(Arc::new(scope))),
            storage,
        }
    }
    pub fn finish(mut self) -> (Vec<Arc<ObservationRecord>>, CaptureStatus) {
        drop(self.guard.take());
        // Check ownership first: an escaping writer could finish between health sampling and this check.
        let unfinished_contexts = Arc::strong_count(&self.storage) != 1;
        let status = CaptureStatus {
            retention: self.storage.memory.stats(),
            emission_errors: self.storage.errors.load(Ordering::Relaxed),
            unfinished_contexts,
        };
        (self.storage.memory.drain_shared(), status)
    }
}
