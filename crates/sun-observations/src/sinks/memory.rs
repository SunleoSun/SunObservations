use crate::{ObservationRecord, Sink};
use parking_lot::Mutex;
use std::io;

#[derive(Default)]
pub struct MemorySink {
    records: Mutex<Vec<ObservationRecord>>,
}
impl MemorySink {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn records(&self) -> Vec<ObservationRecord> {
        self.records.lock().clone()
    }
    pub fn drain(&self) -> Vec<ObservationRecord> {
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
        self.records.lock().push(record.clone());
        Ok(())
    }
}

#[cfg(test)]
#[path = "memory_test.rs"]
mod tests;
