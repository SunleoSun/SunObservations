use super::*;
use crate::{support_test::record, MemorySink};

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
fn one_channel_fans_out_to_multiple_sinks() {
    let channel = Channel::new("logical");
    let a = Arc::new(MemorySink::new());
    let b = Arc::new(MemorySink::new());
    channel.add_sink(a.clone());
    channel.add_sink(b.clone());
    let record = record("channels::event");
    channel.write(&record).unwrap();
    channel.flush().unwrap();
    assert_eq!(a.records(), std::slice::from_ref(&record));
    assert_eq!(b.records(), [record]);
}

#[test]
fn failing_sink_does_not_block_other_targets() {
    let channel = Channel::new("errors");
    let memory = Arc::new(MemorySink::new());
    channel.add_sink(Arc::new(Failing));
    channel.add_sink(memory.clone());
    assert!(channel.write(&record("channels::errors")).is_err());
    assert_eq!(memory.len(), 1);
    assert!(channel.flush().is_err());
}
