use crate::{
    global, observe, start_scope, support_test::capture, Observation, Policy, ScopeConfig,
};
use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Serialize, Observation)]
#[observation(namespace = "macro_tests::lazy", tags("macro_expensive"))]
struct Lazy {
    value: usize,
}

#[test]
fn macro_checks_selection_exclusion_and_policy_before_building_fields() {
    let sink = capture(global(), "macro_tests");
    let calls = AtomicUsize::new(0);
    let value = || calls.fetch_add(1, Ordering::Relaxed);
    assert!(!observe!(Lazy { value: value() }).unwrap());
    {
        let _scope = start_scope(
            ScopeConfig::new("excluded")
                .activate_namespace("macro_tests")
                .exclude_tags(["macro_expensive"]),
        );
        assert!(!observe!(Lazy { value: value() }).unwrap());
    }
    {
        let _scope = start_scope(
            ScopeConfig::new("policy")
                .activate_namespace("macro_tests")
                .override_policy::<Lazy>(Policy::EveryN(2)),
        );
        assert!(!observe!(Lazy { value: value() }).unwrap());
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert!(observe!(Lazy { value: value() },).unwrap());
        assert!(!observe!(Lazy { value: value() }).unwrap());
    }
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(sink.len(), 1);
    assert_eq!(sink.records()[0].scope.as_deref(), Some("policy"));
}
