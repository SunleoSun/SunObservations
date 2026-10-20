use super::*;
use crate::{support_test::record, MemorySink, ObservationEngine};
struct Failing;
impl Sink for Failing {
    fn write(&self, _: &ObservationRecord) -> io::Result<()> {
        Err(io::Error::other("write failed"))
    }
    fn flush(&self) -> io::Result<()> {
        Err(io::Error::other("flush failed"))
    }
}
#[test]
fn one_channel_fans_out_without_deep_record_clones() {
    let engine = ObservationEngine::new();
    let a = Arc::new(MemorySink::new());
    let b = Arc::new(MemorySink::new());
    engine.add_channel_sink("logical", a.clone());
    engine.add_channel_sink("logical", b.clone());
    let sinks: Vec<Arc<dyn Sink>> = vec![a.clone(), b.clone()];
    let record = Arc::new(record("channels::event"));
    dispatch(&sinks, record.clone()).unwrap();
    engine.flush_channels().unwrap();
    assert!(Arc::ptr_eq(&record, &a.shared_records()[0]));
    assert!(Arc::ptr_eq(&a.shared_records()[0], &b.shared_records()[0]));
}
#[test]
fn failing_sink_does_not_block_other_targets() {
    let memory = Arc::new(MemorySink::new());
    let sinks: Vec<Arc<dyn Sink>> = vec![Arc::new(Failing), memory.clone()];
    assert!(dispatch(&sinks, Arc::new(record("channels::errors"))).is_err());
    assert_eq!(memory.len(), 1);
    assert!(flush_sinks(&sinks).is_err());
}
