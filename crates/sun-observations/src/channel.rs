use crate::{ObservationRecord, Sink};
use std::{io, sync::Arc};

/// Immutable logical routing target. Only ObservationEngine mutates its sink graph.
#[derive(Clone)]
pub struct Channel {
    name: String,
    pub(crate) sinks: Vec<Arc<dyn Sink>>,
}
impl Channel {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sinks: Vec::new(),
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
}

pub(crate) fn dispatch(sinks: &[Arc<dyn Sink>], record: Arc<ObservationRecord>) -> io::Result<()> {
    let mut error = None;
    for sink in sinks {
        if let Err(e) = sink.write_shared(record.clone()) {
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
