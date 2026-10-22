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
    let mut certification = sun_observations::certifications::certification(
        "overlapping graph preserves multiplicity identity failure and unique flush",
    );
    let engine = ObservationEngine::isolated();
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
    certification.check_fact_expect(
        "engine.observe_if_active(|| Event).is_err()",
        engine.observe_if_active(|| Event).is_err(),
        true,
    );
    certification.check_fact_expect(
        "engine.sink_error_count()",
        &(engine.sink_error_count()),
        &(1),
    );
    certification.check_fact_expect("a.len()", &(a.len()), &(2));
    certification.check_fact_expect("b.len()", &(b.len()), &(1));
    certification.check_fact_expect("sibling.is_empty()", sibling.is_empty(), true);
    certification.check_fact_expect(
        "Arc::ptr_eq(&a.shared_records()[0], &a.shared_records()[1])",
        Arc::ptr_eq(&a.shared_records()[0], &a.shared_records()[1]),
        true,
    );
    certification.check_fact_expect(
        "Arc::ptr_eq(&a.shared_records()[0], &b.shared_records()[0])",
        Arc::ptr_eq(&a.shared_records()[0], &b.shared_records()[0]),
        true,
    );
    certification.check_fact_expect(
        "engine.flush_channels().is_err()",
        engine.flush_channels().is_err(),
        true,
    );
    certification.check_fact_expect(
        "failing.0.load(Ordering::Relaxed)",
        &(failing.0.load(Ordering::Relaxed)),
        &(1),
    );
    certification.check_fact_expect(
        "engine.sink_error_count()",
        &(engine.sink_error_count()),
        &(2),
    );
    engine.clear_channel_sinks("child");
    certification.check_fact_expect(
        "engine.observe_if_active(|| Event).is_err()",
        engine.observe_if_active(|| Event).is_err(),
        true,
    );
    certification.check_fact_expect("a.len()", &(a.len()), &(3));
    certification.assert_certify();
}

mod dispatch {
    use super::super::*;
    use crate::{test_support::record, MemorySink, ObservationEngine};
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
        let mut certification = sun_observations::certifications::certification(
            "one channel fans out without deep record clones",
        );
        let engine = ObservationEngine::isolated();
        let a = Arc::new(MemorySink::new());
        let b = Arc::new(MemorySink::new());
        engine.add_channel_sink("logical", a.clone());
        engine.add_channel_sink("logical", b.clone());
        let sinks: Vec<Arc<dyn Sink>> = vec![a.clone(), b.clone()];
        let record = Arc::new(record("channels::event"));
        dispatch(&sinks, record.clone()).unwrap();
        engine.flush_channels().unwrap();
        certification.check_fact_expect(
            "Arc::ptr_eq(&record, &a.shared_records()[0])",
            Arc::ptr_eq(&record, &a.shared_records()[0]),
            true,
        );
        certification.check_fact_expect(
            "Arc::ptr_eq(&a.shared_records()[0], &b.shared_records()[0])",
            Arc::ptr_eq(&a.shared_records()[0], &b.shared_records()[0]),
            true,
        );
        certification.assert_certify();
    }
    #[test]
    fn failing_sink_does_not_block_other_targets() {
        let mut certification = sun_observations::certifications::certification(
            "failing sink does not block other targets",
        );
        let memory = Arc::new(MemorySink::new());
        let sinks: Vec<Arc<dyn Sink>> = vec![Arc::new(Failing), memory.clone()];
        certification.check_fact_expect(
            "dispatch(&sinks, Arc::new(record(\"channels::errors\"))).is_err()",
            dispatch(&sinks, Arc::new(record("channels::errors"))).is_err(),
            true,
        );
        certification.check_fact_expect("memory.len()", &(memory.len()), &(1));
        certification.check_fact_expect(
            "flush_sinks(&sinks).is_err()",
            flush_sinks(&sinks).is_err(),
            true,
        );
        certification.assert_certify();
    }
}
