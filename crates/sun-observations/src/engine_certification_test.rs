use crate::*;
use serde::{Serialize, Serializer};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[test]
fn admitted_sink_failure_consumes_budget_and_serialization_failure_reaches_no_sink() {
    struct Failure;
    impl Sink for Failure {
        fn write(&self, _: &ObservationRecord) -> std::io::Result<()> {
            Err(std::io::Error::other("sink"))
        }
    }
    let engine = ObservationEngine::new();
    let memory = Arc::new(MemorySink::new());
    engine.set_selection(Selection::new().activate_all());
    engine
        .register_namespace_channel("admission", "sinks")
        .unwrap();
    engine.add_channel_sink("sinks", Arc::new(Failure));
    engine.add_channel_sink("sinks", memory.clone());
    let _scope =
        start_scope(ScopeConfig::new("budget").override_policy::<Event>(Policy::FirstN(1)));
    assert!(engine.observe_if_active(|| Event).is_err());
    assert!(!engine
        .observe_if_active::<Event, _>(|| panic!("consumed"))
        .unwrap());
    assert_eq!(memory.len(), 1);
    let calls = Arc::new(AtomicUsize::new(0));
    assert!(matches!(
        engine.observe_if_active(|| Probe { calls, fail: true }),
        Err(ObservationError::Serialize(_))
    ));
    assert_eq!(memory.len(), 1);
    let r = &memory.records()[0];
    assert_eq!(r.proc, engine.process_id());
    assert_eq!(r.scope.as_deref(), Some("budget"));
    assert_eq!(r.namespace, Event::NAMESPACE);
    assert_eq!(r.level, ObservationLevel::ERROR);
}

#[derive(Serialize, Observation)]
#[observation(namespace = "admission::event", level = ERROR, tags("detail"))]
struct Event;
#[test]
fn activation_union_and_exclusion_veto_remain_lazy_at_every_boundary() {
    let engine = ObservationEngine::new();
    assert!(!engine
        .observe_if_active::<Event, _>(|| panic!("empty"))
        .unwrap());
    for global in [
        Selection::new().activate_namespace("admission"),
        Selection::new().activate_tags(["detail"]),
    ] {
        engine.set_selection(global);
        assert!(engine.observe_if_active(|| Event).unwrap());
        for local in [
            ScopeConfig::new("veto").exclude_namespace("admission"),
            ScopeConfig::new("veto").exclude_tags(["detail"]),
        ] {
            let _guard = start_scope(local);
            assert!(!engine
                .observe_if_active::<Event, _>(|| panic!("veto"))
                .unwrap());
        }
    }
    for local in [
        ScopeConfig::new("local").activate_namespace("admission"),
        ScopeConfig::new("local").activate_tags(["detail"]),
    ] {
        engine.set_selection(Selection::new());
        let _guard = start_scope(local);
        assert!(engine.observe_if_active(|| Event).unwrap());
        for global in [
            Selection::new().exclude_namespace("admission"),
            Selection::new().exclude_tags(["detail"]),
        ] {
            engine.set_selection(global);
            assert!(!engine
                .observe_if_active::<Event, _>(|| panic!("global veto"))
                .unwrap());
        }
    }
}
struct Probe {
    calls: Arc<AtomicUsize>,
    fail: bool,
}
impl Serialize for Probe {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if self.fail {
            Err(serde::ser::Error::custom("serialization"))
        } else {
            serializer.serialize_u64(17)
        }
    }
}
impl Observation for Probe {
    const NAMESPACE: &'static str = "admission::probe";
    fn message(&self) -> Option<&str> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Some("message")
    }
}
#[test]
fn admitted_failures_consume_policy_but_rejections_do_not_and_sinkless_callbacks_run() {
    let engine = ObservationEngine::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let _scope =
        start_scope(ScopeConfig::new("budget").override_policy::<Probe>(Policy::FirstN(2)));
    assert!(!engine
        .observe_if_active::<Probe, _>(|| panic!("selection"))
        .unwrap());
    engine.set_selection(Selection::new().activate_all());
    assert!(matches!(
        engine.observe_if_active(|| Probe {
            calls: calls.clone(),
            fail: true
        }),
        Err(ObservationError::Serialize(_))
    ));
    assert!(engine
        .observe_if_active(|| Probe {
            calls: calls.clone(),
            fail: false
        })
        .unwrap());
    assert_eq!(calls.load(Ordering::Relaxed), 3);
    assert!(!engine
        .observe_if_active::<Probe, _>(|| panic!("budget"))
        .unwrap());
}
#[test]
fn engine_recreation_cannot_reuse_retired_tls_identity() {
    let mut identities = std::collections::HashSet::new();
    for i in 0..100 {
        let engine = ObservationEngine::new();
        assert!(identities.insert(engine.process_id().to_owned()));
        engine.set_selection(if i % 2 == 0 {
            Selection::new().activate_all()
        } else {
            Selection::new()
        });
        assert_eq!(engine.observe_if_active(|| Event).unwrap(), i % 2 == 0);
    }
}
