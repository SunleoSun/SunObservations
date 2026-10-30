use crate::{
    global, observe, start_scope, test_support::capture, try_observe, Observation, Policy,
    ScopeConfig,
};
use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Serialize, Observation)]
#[observation(namespace = "macro_tests::lazy", tags("macro_expensive"))]
struct Lazy {
    value: usize,
}

#[test]
fn fallible_macro_checks_selection_exclusion_and_policy_before_building_fields() {
    let mut certification = sun_observations::certifications::certification(
        "macro checks selection exclusion and policy before building fields",
    );
    let sink = capture(global(), "macro_tests");
    let calls = AtomicUsize::new(0);
    let value = || calls.fetch_add(1, Ordering::Relaxed);
    certification.check_fact_expect(
        "try_observe!(Lazy { value: value() }).unwrap()",
        try_observe!(Lazy { value: value() }).unwrap(),
        false,
    );
    {
        let _scope = start_scope(
            ScopeConfig::new("excluded")
                .activate_namespace("macro_tests")
                .unwrap()
                .exclude_tags(["macro_expensive"])
                .unwrap(),
        );
        certification.check_fact_expect(
            "try_observe!(Lazy { value: value() }).unwrap()",
            try_observe!(Lazy { value: value() }).unwrap(),
            false,
        );
    }
    {
        let _scope = start_scope(
            ScopeConfig::new("policy")
                .activate_namespace("macro_tests")
                .unwrap()
                .override_policy::<Lazy>(Policy::every_n(2).unwrap()),
        );
        certification.check_fact_expect(
            "try_observe!(Lazy { value: value() }).unwrap()",
            try_observe!(Lazy { value: value() }).unwrap(),
            false,
        );
        certification.check_fact_expect(
            "calls.load(Ordering::Relaxed)",
            &(calls.load(Ordering::Relaxed)),
            &(0),
        );
        certification.check_fact_expect(
            "try_observe!(Lazy { value: value() },).unwrap()",
            try_observe!(Lazy { value: value() },).unwrap(),
            true,
        );
        certification.check_fact_expect(
            "try_observe!(Lazy { value: value() }).unwrap()",
            try_observe!(Lazy { value: value() }).unwrap(),
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

#[test]
fn best_effort_macro_returns_unit_and_preserves_lazy_selection_exclusion_and_policy() {
    let calls = AtomicUsize::new(0);
    let value = || calls.fetch_add(1, Ordering::Relaxed);
    let run = crate::evidence::capture_observations(crate::evidence::CaptureConfig::new(), || {
        observe!(Lazy { value: value() })
    });
    let () = *run.result();
    let mut c = crate::certifications::certification("best-effort lazy emission");
    c.check_fact_expect("disabled construction", calls.load(Ordering::Relaxed), 0);
    c.check_fact_expect("disabled records", run.dataset().records().len(), 0);
    c.check_fact_expect(
        "disabled capture complete",
        run.status().is_complete(),
        true,
    );
    let run = crate::evidence::capture_observations(
        crate::evidence::CaptureConfig::new().activate_all(),
        || {
            {
                let _scope = start_scope(
                    ScopeConfig::new("excluded")
                        .exclude_tags(["macro_expensive"])
                        .unwrap(),
                );
                observe!(Lazy { value: value() });
            }
            c.check_fact_expect("excluded construction", calls.load(Ordering::Relaxed), 0);
            let _scope = start_scope(
                ScopeConfig::new("policy").override_policy::<Lazy>(Policy::every_n(2).unwrap()),
            );
            observe!(Lazy { value: value() });
            c.check_fact_expect(
                "policy rejection construction",
                calls.load(Ordering::Relaxed),
                0,
            );
            let () = observe!(Lazy { value: value() },);
            observe!(Lazy { value: value() });
        },
    );
    c.check_fact_expect("admitted construction", calls.load(Ordering::Relaxed), 1);
    c.check_fact_expect("admitted records", run.dataset().records().len(), 1);
    c.check_fact_expect(
        "admitted capture complete",
        run.status().is_complete(),
        true,
    );
    c.assert_certify();
}

#[test]
fn both_macros_preserve_sink_errors_in_runtime_health_without_forcing_caller_failure() {
    struct FailingSink(AtomicUsize);
    impl crate::Sink for FailingSink {
        fn write(&self, _: &crate::ObservationRecord) -> std::io::Result<()> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Err(std::io::Error::other("macro sink failure"))
        }
        fn flush(&self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[derive(Serialize, Observation)]
    #[observation(namespace = "macro_sink_failure::event")]
    struct Event;
    let engine = global();
    let sink = std::sync::Arc::new(FailingSink(AtomicUsize::new(0)));
    engine.add_channel_sink("macro_sink_failure", sink.clone());
    engine
        .register_namespace_channel("macro_sink_failure", "macro_sink_failure")
        .unwrap();
    let _scope = start_scope(
        ScopeConfig::new("sink failure")
            .activate_namespace("macro_sink_failure")
            .unwrap(),
    );
    let before = engine.sink_error_count();
    let result: Result<bool, crate::ObservationError> = try_observe!(Event,);
    let after_try = engine.sink_error_count();
    let caller_result = (|| {
        observe!(Event);
        Ok::<_, &'static str>("continued")
    })();
    let after_observe = engine.sink_error_count();
    engine.clear_channel_sinks("macro_sink_failure");
    let mut c = crate::certifications::certification("macro sink failure contracts");
    c.check_fact_expect(
        "fallible sink category",
        matches!(result, Err(crate::ObservationError::Sink(_))),
        true,
    );
    c.check_fact_expect("fallible health increment", after_try, before + 1);
    c.check_fact_expect("best-effort health increment", after_observe, after_try + 1);
    c.check_fact_expect(
        "both dispatches attempted",
        sink.0.load(Ordering::Relaxed),
        2,
    );
    c.check_fact_expect("caller continues", caller_result, Ok("continued"));
    c.assert_certify();
}
