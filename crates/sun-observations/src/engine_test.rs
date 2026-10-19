use super::*;
use crate::{start_scope, support_test::capture, MemorySink};
use serde::{Serialize, Serializer};

#[derive(Serialize, crate::Observation)]
#[observation(namespace = "engine_tests::train::epoch", level = WARN, tags("detail"))]
struct Event {
    epoch: u32,
}

#[test]
fn global_and_scope_selectors_union_but_either_exclusion_vetoes() {
    let engine = ObservationEngine::new();
    let sink = capture(&engine, "engine_tests");
    engine.set_selection(Selection::new().activate_tags(["detail"]));
    assert!(engine.observe_if_active(|| Event { epoch: 1 }).unwrap());
    {
        let _scope =
            start_scope(ScopeConfig::new("exclude").exclude_namespace("engine_tests::train"));
        assert!(!engine.observe_if_active(|| Event { epoch: 2 }).unwrap());
    }
    engine.set_selection(Selection::new().exclude_tags(["detail"]));
    {
        let _scope = start_scope(ScopeConfig::new("active").activate_namespace("engine_tests"));
        assert!(!engine.observe_if_active(|| Event { epoch: 3 }).unwrap());
    }
    engine.set_selection(Selection::new());
    {
        let _scope = start_scope(ScopeConfig::new("scope-only").activate_namespace("engine_tests"));
        assert!(engine.observe_if_active(|| Event { epoch: 4 }).unwrap());
    }
    let records = sink.records();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].scope, None);
    assert_eq!(records[1].scope.as_deref(), Some("scope-only"));
    assert_eq!(records[1].level, ObservationLevel::WARN);
}

#[test]
fn namespace_routing_fans_out_and_envelope_uses_engine_identity_and_clocks() {
    let engine = ObservationEngine::new();
    let all = capture(&engine, "engine_tests");
    let train = capture(&engine, "engine_tests::train");
    let sibling = capture(&engine, "engine_tests::infer");
    engine.set_selection(Selection::new().activate_namespace("engine_tests"));
    engine.observe_if_active(|| Event { epoch: 7 }).unwrap();
    let record = all.records().pop().unwrap();
    assert_eq!(record.proc, engine.process_id());
    assert!(chrono::DateTime::parse_from_rfc3339(&record.ts).is_ok());
    assert_eq!(record.data, serde_json::json!({"epoch": 7}));
    assert_eq!(train.records(), std::slice::from_ref(&record));
    assert!(sibling.is_empty());
    engine.observe_if_active(|| Event { epoch: 8 }).unwrap();
    assert!(all.records()[1].proc_ms >= record.proc_ms);
    assert_ne!(engine.process_id(), ObservationEngine::new().process_id());
}

#[test]
fn warn_does_not_bypass_selection() {
    let engine = ObservationEngine::new();
    assert!(!engine
        .observe_if_active::<Event, _>(|| panic!("must stay lazy"))
        .unwrap());
}

#[test]
fn serialization_failure_is_returned_without_writing_a_record() {
    struct Bad;
    impl Serialize for Bad {
        fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("deliberate failure"))
        }
    }
    impl Observation for Bad {
        const NAMESPACE: &'static str = "engine_tests::bad";
    }
    let engine = ObservationEngine::new();
    let sink = capture(&engine, "engine_tests");
    engine.set_selection(Selection::new().activate_all());
    assert!(matches!(
        engine.observe_if_active(|| Bad),
        Err(ObservationError::Serialize(_))
    ));
    assert!(sink.is_empty());
}

#[test]
fn message_works_without_registration_and_policy_defaults_to_always() {
    #[derive(Serialize)]
    struct Message {
        text: &'static str,
    }
    impl Observation for Message {
        const NAMESPACE: &'static str = "engine_tests::message";
        fn message(&self) -> Option<&str> {
            Some(self.text)
        }
    }
    let engine = ObservationEngine::new();
    let sink = capture(&engine, "engine_tests");
    engine.set_selection(Selection::new().activate_all());
    assert!(engine
        .observe_if_active(|| Message { text: "hello" })
        .unwrap());
    assert!(engine
        .observe_if_active(|| Message { text: "again" })
        .unwrap());
    let records = sink.records();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].message.as_deref(), Some("hello"));
    assert_eq!(records[0].data, serde_json::json!({"text": "hello"}));
    assert_eq!(records[1].message.as_deref(), Some("again"));
    {
        let _scope = start_scope(
            ScopeConfig::new("override").override_policy::<Message>(crate::Policy::Never),
        );
        assert!(!engine
            .observe_if_active::<Message, _>(|| panic!("scope policy must reject before build"))
            .unwrap());
    }
}

#[test]
fn sink_failures_are_counted_and_other_channels_still_receive() {
    struct Broken;
    impl Sink for Broken {
        fn write(&self, _: &ObservationRecord) -> io::Result<()> {
            Err(io::Error::other("broken"))
        }
        fn flush(&self) -> io::Result<()> {
            Err(io::Error::other("broken"))
        }
    }
    let engine = ObservationEngine::new();
    let memory = Arc::new(MemorySink::new());
    engine.channel("broken").add_sink(Arc::new(Broken));
    engine.channel("healthy").add_sink(memory.clone());
    engine
        .register_namespace_channel("engine_tests", "broken")
        .unwrap();
    engine
        .register_namespace_channel("engine_tests", "healthy")
        .unwrap();
    engine.set_selection(Selection::new().activate_all());
    assert!(matches!(
        engine.observe_if_active(|| Event { epoch: 1 }),
        Err(ObservationError::Sink(_))
    ));
    assert_eq!(memory.len(), 1);
    assert_eq!(engine.sink_error_count(), 1);
    assert!(engine.flush().is_err());
    assert_eq!(engine.sink_error_count(), 2);
}
