use crate::{
    global, observe_span, start_scope, support_test::capture, Observation, ObservationEngine,
    Phase, Policy, ScopeConfig, Selection, Sink,
};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Serialize, Observation)]
#[observation(namespace = "span_tests::fit")]
struct Fit {
    samples: usize,
}

#[test]
fn start_end_share_identity_scope_and_frozen_routing() {
    let engine = ObservationEngine::new();
    let sink = capture(&engine, "span_tests");
    let scope = start_scope(ScopeConfig::new("fit-run").activate_namespace("span_tests"));
    let span = engine.span_if_active(|| Fit { samples: 42 }).unwrap();
    assert!(span.is_active());
    assert_eq!(sink.len(), 1);
    drop(scope);
    engine.set_selection(Selection::new());
    let later = Arc::new(crate::MemorySink::new());
    engine.add_channel_sink("span_tests", later.clone());
    drop(span);
    let records = sink.records();
    assert_eq!(records.len(), 2);
    assert!(later.is_empty());
    let (start, end) = (&records[0], &records[1]);
    assert_eq!(start.phase, Some(Phase::Start));
    assert_eq!(end.phase, Some(Phase::End));
    assert_eq!(start.namespace, end.namespace);
    assert_eq!(start.scope.as_deref(), Some("fit-run"));
    assert_eq!(start.scope, end.scope);
    assert!(start.span.is_some());
    assert_eq!(start.span, end.span);
    assert_eq!(start.proc, end.proc);
    assert!(end.proc_ms >= start.proc_ms);
    assert_eq!(start.data, serde_json::json!({"samples": 42}));
    assert!(end.data.is_null());
}

#[test]
fn rejected_spans_are_lazy_and_have_no_partial_lifecycle() {
    let engine = ObservationEngine::new();
    let sink = capture(&engine, "span_tests");
    let calls = AtomicUsize::new(0);
    let build = || Fit {
        samples: calls.fetch_add(1, Ordering::Relaxed),
    };
    assert!(!engine.span_if_active(build).unwrap().is_active());
    {
        let _scope = start_scope(
            ScopeConfig::new("never")
                .activate_namespace("span_tests")
                .override_policy::<Fit>(Policy::Never),
        );
        assert!(!engine.span_if_active(build).unwrap().is_active());
    }
    {
        let _scope = start_scope(
            ScopeConfig::new("excluded")
                .activate_namespace("span_tests")
                .exclude_namespace("span_tests"),
        );
        assert!(!engine.span_if_active(build).unwrap().is_active());
    }
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert!(sink.is_empty());
}

#[test]
fn macro_spans_have_distinct_ids_and_end_during_unwind() {
    let sink = capture(global(), "span_tests::fit");
    let _scope = start_scope(ScopeConfig::new("unwind").activate_namespace("span_tests"));
    let result = std::panic::catch_unwind(|| {
        let _a = observe_span!(Fit { samples: 1 }).unwrap();
        let _b = observe_span!(Fit { samples: 2 }).unwrap();
        panic!("unwind");
    });
    assert!(result.is_err());
    let records = sink.records();
    assert_eq!(records.len(), 4);
    assert_ne!(records[0].span, records[1].span);
    assert_eq!(records[0].span, records[3].span);
    assert_eq!(records[1].span, records[2].span);
    assert_eq!(records[3].phase, Some(Phase::End));
}

#[test]
fn policy_counts_one_span_not_its_lifecycle_records() {
    let engine = ObservationEngine::new();
    let sink = capture(&engine, "span_tests");
    let _scope = start_scope(
        ScopeConfig::new("policy")
            .activate_namespace("span_tests")
            .override_policy::<Fit>(Policy::EveryN(2)),
    );
    for accepted in [false, true, false, true] {
        let span = engine.span_if_active(|| Fit { samples: 10 }).unwrap();
        assert_eq!(span.is_active(), accepted);
        span.finish().unwrap();
    }
    assert_eq!(sink.len(), 4);
    assert_eq!(
        sink.records()
            .iter()
            .filter(|r| r.phase == Some(Phase::End))
            .count(),
        2
    );
}

#[test]
fn explicit_finish_reports_end_error_and_never_retries() {
    struct FailEnd(AtomicUsize);
    impl Sink for FailEnd {
        fn write(&self, _: &crate::ObservationRecord) -> std::io::Result<()> {
            if self.0.fetch_add(1, Ordering::Relaxed) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::other("end failed"))
            }
        }
    }
    let engine = ObservationEngine::new();
    let sink = Arc::new(FailEnd(AtomicUsize::new(0)));
    engine.add_channel_sink("end", sink.clone());
    engine
        .register_namespace_channel("span_tests", "end")
        .unwrap();
    engine.set_selection(Selection::new().activate_all());
    assert!(engine
        .span_if_active(|| Fit { samples: 1 })
        .unwrap()
        .finish()
        .is_err());
    assert_eq!(sink.0.load(Ordering::Relaxed), 2);
    assert_eq!(engine.sink_error_count(), 1);
}
