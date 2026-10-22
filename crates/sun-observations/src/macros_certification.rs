use crate::{
    global, observe, start_scope, test_support::capture, Observation, Policy, ScopeConfig,
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
    let mut certification = sun_certifications::certify(
        "macro checks selection exclusion and policy before building fields",
    );
    let sink = capture(global(), "macro_tests");
    let calls = AtomicUsize::new(0);
    let value = || calls.fetch_add(1, Ordering::Relaxed);
    certification.check_fact_expect(
        "observe!(Lazy { value: value() }).unwrap()",
        observe!(Lazy { value: value() }).unwrap(),
        false,
    );
    {
        let _scope = start_scope(
            ScopeConfig::new("excluded")
                .activate_namespace("macro_tests")
                .exclude_tags(["macro_expensive"]),
        );
        certification.check_fact_expect(
            "observe!(Lazy { value: value() }).unwrap()",
            observe!(Lazy { value: value() }).unwrap(),
            false,
        );
    }
    {
        let _scope = start_scope(
            ScopeConfig::new("policy")
                .activate_namespace("macro_tests")
                .override_policy::<Lazy>(Policy::EveryN(2)),
        );
        certification.check_fact_expect(
            "observe!(Lazy { value: value() }).unwrap()",
            observe!(Lazy { value: value() }).unwrap(),
            false,
        );
        certification.check_fact_expect(
            "calls.load(Ordering::Relaxed)",
            &(calls.load(Ordering::Relaxed)),
            &(0),
        );
        certification.check_fact_expect(
            "observe!(Lazy { value: value() },).unwrap()",
            observe!(Lazy { value: value() },).unwrap(),
            true,
        );
        certification.check_fact_expect(
            "observe!(Lazy { value: value() }).unwrap()",
            observe!(Lazy { value: value() }).unwrap(),
            false,
        );
    }
    certification.check_fact_expect(
        "calls.load(Ordering::Relaxed)",
        &(calls.load(Ordering::Relaxed)),
        &(1),
    );
    certification.check_fact_expect("sink.len()", &(sink.len()), &(1));
    certification.check_fact_expect(
        "sink.records()[0].scope.as_deref()",
        &(sink.records()[0].scope.as_deref()),
        &(Some("policy")),
    );
    certification.assert_certify();
}
