use crate::{ObservationRecord, Sink};
use parking_lot::Mutex;
use std::{io, sync::Arc};

#[derive(Default)]
pub struct MemorySink {
    records: Mutex<Vec<Arc<ObservationRecord>>>,
}
impl MemorySink {
    pub fn new() -> Self {
        Self::default()
    }
    /// Owned compatibility snapshot. Prefer shared_records() for large captures.
    pub fn records(&self) -> Vec<ObservationRecord> {
        self.records.lock().iter().map(|r| (**r).clone()).collect()
    }
    pub fn shared_records(&self) -> Vec<Arc<ObservationRecord>> {
        self.records.lock().clone()
    }
    pub fn drain(&self) -> Vec<ObservationRecord> {
        self.drain_shared()
            .into_iter()
            .map(Arc::unwrap_or_clone)
            .collect()
    }
    pub fn drain_shared(&self) -> Vec<Arc<ObservationRecord>> {
        std::mem::take(&mut *self.records.lock())
    }
    pub fn len(&self) -> usize {
        self.records.lock().len()
    }
    pub fn is_empty(&self) -> bool {
        self.records.lock().is_empty()
    }
}
impl Sink for MemorySink {
    fn write(&self, record: &ObservationRecord) -> io::Result<()> {
        self.write_shared(Arc::new(record.clone()))
    }
    fn write_shared(&self, record: Arc<ObservationRecord>) -> io::Result<()> {
        self.records.lock().push(record);
        Ok(())
    }
}

#[cfg(test)]
#[path = "memory_test.rs"]
mod tests;
