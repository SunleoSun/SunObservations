use crate::{ObservationRecord, Sink};
use std::{io, sync::Arc};

/// Immutable logical routing target. Only ObservationEngine mutates its sink graph.
#[derive(Clone)]
pub struct Channel {
    pub(crate) sinks: Vec<Arc<dyn Sink>>,
    pub(crate) needs_render_text: bool,
}
impl Channel {
    pub(crate) fn new() -> Self {
        Self {
            sinks: Vec::new(),
            needs_render_text: false,
        }
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
pub(crate) fn dispatch_delivery(
    sinks: &[Arc<dyn Sink>],
    delivery: &crate::ObservationDelivery,
) -> io::Result<()> {
    let mut error = None;
    for sink in sinks {
        if let Err(e) = sink.write_delivery(delivery) {
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
#[path = "channel_certification.rs"]
mod certification;
