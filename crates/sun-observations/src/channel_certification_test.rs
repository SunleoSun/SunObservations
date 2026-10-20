use crate::*;
use serde::Serialize;
use std::{
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
#[derive(Serialize, Observation)]
#[observation(namespace = "graph::child::event")]
struct Event;
struct Failing(AtomicUsize);
impl Sink for Failing {
    fn write(&self, _: &ObservationRecord) -> io::Result<()> {
        Err(io::Error::other("delivery"))
    }
    fn flush(&self) -> io::Result<()> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Err(io::Error::other("flush"))
    }
}
#[test]
fn overlapping_graph_preserves_multiplicity_identity_failure_and_unique_flush() {
    let engine = ObservationEngine::new();
    engine.set_selection(Selection::new().activate_all());
    let a = Arc::new(MemorySink::new());
    let b = Arc::new(MemorySink::new());
    let sibling = Arc::new(MemorySink::new());
    let failing = Arc::new(Failing(AtomicUsize::new(0)));
    for channel in ["parent", "child"] {
        engine.add_channel_sink(channel, a.clone());
        engine.add_channel_sink(channel, failing.clone());
    }
    engine.add_channel_sink("parent", b.clone());
    engine.add_channel_sink("sibling", sibling.clone());
    for (namespace, channel) in [
        ("graph", "parent"),
        ("graph", "parent"),
        ("graph::child", "child"),
        ("graph::children", "sibling"),
    ] {
        engine
            .register_namespace_channel(namespace, channel)
            .unwrap();
    }
    assert!(engine.observe_if_active(|| Event).is_err());
    assert_eq!(engine.sink_error_count(), 1);
    assert_eq!(a.len(), 2);
    assert_eq!(b.len(), 1);
    assert!(sibling.is_empty());
    assert!(Arc::ptr_eq(&a.shared_records()[0], &a.shared_records()[1]));
    assert!(Arc::ptr_eq(&a.shared_records()[0], &b.shared_records()[0]));
    assert!(engine.flush_channels().is_err());
    assert_eq!(failing.0.load(Ordering::Relaxed), 1);
    assert_eq!(engine.sink_error_count(), 2);
    engine.clear_channel_sinks("child");
    assert!(engine.observe_if_active(|| Event).is_err());
    assert_eq!(a.len(), 3);
}
