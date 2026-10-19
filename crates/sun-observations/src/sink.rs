use crate::ObservationRecord;
use std::io;

/// Implementations must serialize concurrent writes safely. No engine locks are held here.
pub trait Sink: Send + Sync + 'static {
    fn write(&self, record: &ObservationRecord) -> io::Result<()>;
    fn flush(&self) -> io::Result<()> {
        Ok(())
    }
}
