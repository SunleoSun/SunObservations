use crate::ObservationRecord;
use std::{io, sync::Arc};

/// Callbacks run without engine/configuration locks. Implementations serialize concurrent writes.
pub trait Sink: Send + Sync + 'static {
    /// Convenience path for callers holding a borrowed record.
    fn write(&self, record: &ObservationRecord) -> io::Result<()>;
    /// Engine fan-out shares one immutable allocation. Override to avoid deep cloning.
    fn write_shared(&self, record: Arc<ObservationRecord>) -> io::Result<()> {
        self.write(&record)
    }
    fn flush(&self) -> io::Result<()> {
        Ok(())
    }
}
