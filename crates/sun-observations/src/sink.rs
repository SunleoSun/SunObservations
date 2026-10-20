use crate::ObservationRecord;
use std::{io, sync::Arc};

/// Stack-owned delivery; cloning shares the canonical record and optional transient text.
/// Arc<String> retains the renderer's buffer without copying it into an Arc<str> allocation.
#[derive(Clone, Debug)]
pub struct ObservationDelivery {
    record: Arc<ObservationRecord>,
    render_text: Option<Arc<String>>,
}
impl ObservationDelivery {
    pub fn new(record: Arc<ObservationRecord>, render_text: Option<Arc<String>>) -> Self {
        Self {
            record,
            render_text,
        }
    }
    pub fn record(&self) -> &ObservationRecord {
        &self.record
    }
    pub fn shared_record(&self) -> &Arc<ObservationRecord> {
        &self.record
    }
    pub fn render_text(&self) -> Option<&str> {
        self.render_text.as_deref().map(String::as_str)
    }
    pub fn shared_render_text(&self) -> Option<&Arc<String>> {
        self.render_text.as_ref()
    }
}

/// Callbacks run without engine/configuration locks. Implementations serialize concurrent writes.
pub trait Sink: Send + Sync + 'static {
    /// Convenience path for callers holding a borrowed record.
    fn write(&self, record: &ObservationRecord) -> io::Result<()>;
    /// Engine fan-out shares one immutable allocation. Override to avoid deep cloning.
    fn write_shared(&self, record: Arc<ObservationRecord>) -> io::Result<()> {
        self.write(&record)
    }
    /// Sampled once at attachment, without engine locks. Immutable while attached;
    /// routing plans aggregate the published capability, never query it on warm emission.
    fn wants_render_text(&self) -> bool {
        false
    }
    /// Human-capable fan-out. The default preserves existing structured sink implementations.
    fn write_delivery(&self, delivery: &ObservationDelivery) -> io::Result<()> {
        self.write_shared(delivery.shared_record().clone())
    }
    fn flush(&self) -> io::Result<()> {
        Ok(())
    }
}
