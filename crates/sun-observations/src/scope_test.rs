use super::*;
use crate::{support_test::capture, ObservationEngine};
use serde::Serialize;

#[derive(Serialize, crate::Observation)]
#[observation(namespace = "scope_tests::event")]
struct Event {
    value: u32,
}

#[derive(Serialize, crate::Observation)]
#[observation(namespace = "scope_tests::other")]
struct Other;

#[test]
fn counters_are_isolated_between_scopes_even_with_equal_names() {
    let engine = ObservationEngine::new();
    let sink = capture(&engine, "scope_tests");
    let config = ScopeConfig::new("same-name")
        .activate_namespace("scope_tests")
        .override_policy::<Event>(Policy::EveryN(2));
    for _ in 0..2 {
        let _scope = start_scope(config.clone());
        assert!(!engine.observe_if_active(|| Event { value: 1 }).unwrap());
        assert!(engine.observe_if_active(|| Event { value: 2 }).unwrap());
        assert!(engine.observe_if_active(|| Other).unwrap());
    }
    assert_eq!(sink.len(), 4);
}

#[test]
fn first_n_and_max_per_scope_reset_and_overrides_do_not_escape() {
    let engine = ObservationEngine::new();
    let sink = capture(&engine, "scope_tests");
    engine.set_selection(Selection::new().activate_namespace("scope_tests"));
    for policy in [Policy::FirstN(1), Policy::MaxPerScope(1), Policy::Never] {
        for name in ["a", "b"] {
            let _scope = start_scope(ScopeConfig::new(name).override_policy::<Event>(policy));
            assert_eq!(
                engine.observe_if_active(|| Event { value: 1 }).unwrap(),
                policy != Policy::Never
            );
            assert!(!engine.observe_if_active(|| Event { value: 2 }).unwrap());
        }
    }
    assert!(engine.observe_if_active(|| Event { value: 3 }).unwrap());
    assert_eq!(sink.len(), 5);
    assert_eq!(sink.records().last().unwrap().scope, None);
}

#[test]
fn filtered_occurrences_do_not_advance_policy() {
    let engine = ObservationEngine::new();
    let _scope =
        start_scope(ScopeConfig::new("policy").override_policy::<Event>(Policy::EveryN(2)));
    assert!(!engine.observe_if_active(|| Event { value: 1 }).unwrap());
    engine.set_selection(Selection::new().activate_namespace("scope_tests"));
    assert!(!engine.observe_if_active(|| Event { value: 2 }).unwrap());
    assert!(engine.observe_if_active(|| Event { value: 3 }).unwrap());
}
