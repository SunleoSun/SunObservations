use crate::{ObservationRecord, Sink};
use parking_lot::RwLock;
use std::{io, sync::Arc};

/// A logical route target with any number of sinks, not a file abstraction.
pub struct Channel {
    name: String,
    sinks: RwLock<Vec<Arc<dyn Sink>>>,
}
impl Channel {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sinks: RwLock::new(Vec::new()),
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn add_sink(&self, sink: Arc<dyn Sink>) {
        self.sinks.write().push(sink);
    }
    pub(crate) fn sinks(&self) -> Vec<Arc<dyn Sink>> {
        self.sinks.read().clone()
    }
    pub fn write(&self, record: &ObservationRecord) -> io::Result<()> {
        dispatch(&self.sinks(), record)
    }
    pub fn flush(&self) -> io::Result<()> {
        flush_sinks(&self.sinks())
    }
}

// Always attempt every target, even when an earlier target fails.
pub(crate) fn dispatch(sinks: &[Arc<dyn Sink>], record: &ObservationRecord) -> io::Result<()> {
    let mut error = None;
    for sink in sinks {
        if let Err(e) = sink.write(record) {
            error.get_or_insert(e);
        }
    }
    error.map_or(Ok(()), Err)
}
pub(crate) fn flush_sinks(sinks: &[Arc<dyn Sink>]) -> io::Result<()> {
    let mut error = None;
    for sink in sinks {
        if let Err(e) = sink.flush() {
            error.get_or_insert(e);
        }
    }
    error.map_or(Ok(()), Err)
}

#[cfg(test)]
#[path = "channel_test.rs"]
mod tests;
