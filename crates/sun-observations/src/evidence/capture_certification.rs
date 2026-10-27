use super::*;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Barrier};
use sun_observations::certifications::*;
use sun_observations::{observe, observe_span, start_scope, ObservationError, ScopeConfig};
#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "capture::event", tags("evidence"))]
struct Event {
    run: u64,
    row: u64,
}
fn execute(run: u64, n: u64) -> Result<u64, ObservationError> {
    for row in 0..n {
        observe!(Event { run, row })?;
    }
    Ok(n)
}
#[derive(Serialize, Deserialize)]
struct InvalidManual;
impl sun_observations::Observation for InvalidManual {
    const NAMESPACE: &'static str = "Invalid::Name";
}
#[derive(Serialize, Deserialize)]
struct ValidManual;
impl sun_observations::Observation for ValidManual {
    const NAMESPACE: &'static str = "manual::valid";
}
#[derive(Serialize)]
struct InvalidTagManual;
impl sun_observations::Observation for InvalidTagManual {
    const NAMESPACE: &'static str = "manual::tag";
    const TAGS: &'static [&'static str] = &["Invalid"];
}

#[test]
fn capture_respects_enclosing_scope_identity_exclusions_and_counter_lifecycle() {
    use sun_observations::Policy;
    let unscoped = capture_observations(config(), || {
        assert!(sun_observations::Context::capture().scope_name().is_none());
        observe!(Event { run: 0, row: 0 }).unwrap()
    });
    assert!(*unscoped.result());
    assert!(unscoped.dataset().records()[0].scope.is_none());
    {
        let _scope = start_scope(
            ScopeConfig::new("job-42")
                .activate_all()
                .override_policy::<Event>(Policy::Never),
        );
        assert!(!observe!(Event { run: 0, row: 0 }).unwrap());
        let run = capture_observations(CaptureConfig::new().activate_all(), || {
            observe!(Event { run: 0, row: 1 })
        });
        assert!(!run.result().as_ref().unwrap());
        assert!(run.dataset().records().is_empty());
        assert!(run.status().is_complete());
    }
    {
        let _scope = start_scope(
            ScopeConfig::new("job-42")
                .activate_all()
                .override_policy::<Event>(Policy::first_n(2).unwrap()),
        );
        assert!(observe!(Event { run: 1, row: 0 }).unwrap());
        let run = capture_observations(CaptureConfig::new().activate_all(), || {
            (
                observe!(Event { run: 1, row: 1 }).unwrap(),
                observe!(Event { run: 1, row: 2 }).unwrap(),
            )
        });
        assert_eq!(*run.result(), (true, false));
        assert_eq!(run.dataset().records().len(), 1);
        assert_eq!(run.dataset().records()[0].scope.as_deref(), Some("job-42"));
        assert!(run.status().is_complete());
        assert!(!observe!(Event { run: 1, row: 3 }).unwrap());
    }
    {
        let _scope = start_scope(
            ScopeConfig::new("excluded")
                .exclude_namespace("capture")
                .unwrap(),
        );
        let run = capture_observations(config(), || observe!(Event { run: 2, row: 0 }));
        assert!(!run.result().as_ref().unwrap());
        assert!(run.dataset().records().is_empty());
    }
}

#[test]
fn manual_metadata_is_validated_before_capture_and_persisted_ingestion() {
    let bad = capture_observations(CaptureConfig::new().activate_all(), || {
        observe!(InvalidManual)
    });
    assert!(matches!(
        bad.result(),
        Err(ObservationError::InvalidNamespace(_))
    ));
    assert!(!bad.status().is_complete());
    assert!(bad.dataset().records().is_empty());
    let bad_tag = capture_observations(CaptureConfig::new().activate_all(), || {
        observe!(InvalidTagManual)
    });
    assert!(matches!(
        bad_tag.result(),
        Err(ObservationError::InvalidTag(_))
    ));
    assert!(!bad_tag.status().is_complete());
    let good = capture_observations(CaptureConfig::new().activate_all(), || {
        observe!(ValidManual)
    });
    assert_eq!(*good.result().as_ref().unwrap(), true);
    assert!(good.status().is_complete());
    assert_eq!(good.to_typed::<ValidManual>().unwrap().values().len(), 1);
    assert!(ObservationDataset::try_from_records(good.dataset().records().to_vec()).is_ok());
    let mut invalid_record = good.dataset().records()[0].as_ref().clone();
    invalid_record.namespace = "Invalid::Name".into();
    assert!(ObservationDataset::try_from_records(vec![Arc::new(invalid_record)]).is_err());
    let bad_span = capture_observations(CaptureConfig::new().activate_all(), || {
        observe_span!(InvalidManual).is_err()
    });
    assert!(*bad_span.result());
    assert!(!bad_span.status().is_complete());
    assert!(bad_span.dataset().records().is_empty());
}

#[test]
fn scoped_capture_reuses_runtime_lazy_policy_and_span_lifecycle() {
    let built = std::cell::Cell::new(0u64);
    let run = capture_observations(config(), || {
        let _scope = start_scope(
            ScopeConfig::new("budget")
                .override_policy::<Event>(sun_observations::Policy::first_n(2).unwrap()),
        );
        for row in 0..10 {
            observe!(Event {
                run: {
                    built.set(built.get() + 1);
                    1
                },
                row
            })
            .unwrap();
        }
    });
    let span = capture_observations(config(), || {
        let span = observe_span!(Event { run: 1, row: 1 }).unwrap();
        span.finish().unwrap();
    });
    run.certification("canonical capture admission")
        .check_fact_expect("lazy policy builds", built.get(), 2u64)
        .check_fact_expect(
            "policy records",
            run.to_typed::<Event>().unwrap().values().len(),
            2usize,
        )
        .check_fact_expect("complete span", span.status().is_complete(), true)
        .check_fact_expect("both span halves", span.dataset().records().len(), 2usize)
        .check_fact_expect(
            "only start decodes",
            span.to_typed::<Event>().unwrap().values().len(),
            1usize,
        )
        .assert_certify();
}
fn config() -> CaptureConfig {
    CaptureConfig::new().activate_tag("evidence").unwrap()
}
#[test]
fn capture_preserves_results_and_canonical_selection_with_exclusions() {
    let run = capture_observations(config(), || execute(7, 64));
    let typed = run.to_typed::<Event>().unwrap();
    let mut c = run.certification("ordinary production capture");
    c.check_fact_expect("result", *run.result().as_ref().unwrap(), 64u64);
    c.check("count", typed.source("events").count().expect(64usize));
    c.check(
        "identity",
        typed
            .source("events")
            .unique(|v| (v.run, v.row))
            .expect_valid(),
    );
    c.check_fact_expect(
        "healthy capture contributes explicit completeness prerequisite",
        run.certification("health").report().checks(),
        1usize,
    );
    for selection in [
        CaptureConfig::new().activate_namespace("capture").unwrap(),
        CaptureConfig::new()
            .activate_namespaces(["other", "capture::event"])
            .unwrap()
            .activate_tags(["other"])
            .unwrap(),
    ] {
        let selected = capture_observations(selection, || execute(1, 1));
        c.check_fact_expect(
            "canonical namespace",
            selected.to_typed::<Event>().unwrap().values().len(),
            1usize,
        );
    }
    for selection in [
        config().exclude_namespace("capture").unwrap(),
        config().exclude_namespaces(["capture::event"]).unwrap(),
        config().exclude_tag("evidence").unwrap(),
        config().exclude_tags(["evidence"]).unwrap(),
        CaptureConfig::new().activate_namespace("cap").unwrap(),
    ] {
        let excluded = capture_observations(selection, || execute(1, 1));
        c.check_fact_expect(
            "canonical exclusion/boundary",
            excluded.to_typed::<Event>().unwrap().values().len(),
            0usize,
        );
    }
    c.assert_certify();
}
#[test]
fn parallel_propagated_captures_never_mix_evidence() {
    let barrier = Arc::new(Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|id| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let run = capture_observations(config(), || {
                    barrier.wait();
                    let child = sun_observations::context::thread::spawn(move || execute(id, 128));
                    child.join().unwrap().unwrap()
                });
                let typed = run.to_typed::<Event>().unwrap();
                run.certification("parallel capture")
                    .check_fact_expect("result", *run.result(), 128u64)
                    .check("count", typed.source("events").count().expect(128usize))
                    .check(
                        "isolation",
                        typed
                            .source("events")
                            .matching_count(|v| v.run != id)
                            .expect(0usize),
                    )
                    .assert_certify();
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
}
#[test]
fn nested_capture_is_exclusive_and_child_scopes_inherit_attachment() {
    let outer = capture_observations(config(), || {
        execute(1, 1).unwrap();
        let inner = capture_observations(config(), || execute(2, 3));
        inner
            .certification("inner")
            .check_fact_expect(
                "only inner",
                inner.to_typed::<Event>().unwrap().values().len(),
                3usize,
            )
            .assert_certify();
        let _scope = start_scope(ScopeConfig::new("child"));
        execute(1, 2).unwrap();
    });
    let typed = outer.to_typed::<Event>().unwrap();
    outer
        .certification("outer")
        .check_fact_expect("own evidence", typed.values().len(), 3usize)
        .check(
            "no nested evidence",
            typed
                .source("outer")
                .matching_count(|v| v.run == 2)
                .expect(0usize),
        )
        .assert_certify();
}
#[test]
fn capture_cleans_up_on_error_and_unwind_without_assertion() {
    let error = capture_observations(config(), || -> Result<(), &'static str> {
        execute(1, 1).map_err(|_| "emit")?;
        Err("application")
    });
    let panic = std::panic::catch_unwind(|| {
        capture_observations(config(), || {
            execute(9, 1).unwrap();
            panic!("unwind")
        })
    });
    let next = capture_observations(config(), || execute(3, 1));
    next.certification("RAII cleanup")
        .check_fact_expect(
            "application preserved",
            error.result().as_ref().err().copied(),
            Some("application"),
        )
        .check_fact_expect("panic propagated", panic.is_err(), true)
        .check_fact_expect(
            "clean next capture",
            next.to_typed::<Event>().unwrap().values()[0].run,
            3u64,
        )
        .assert_certify();
}
#[test]
fn eviction_and_escaped_contexts_automatically_block_proof() {
    let run = capture_observations(
        config().retention(MemorySinkConfig::new(10, 0.2).unwrap()),
        || execute(1, 40),
    );
    let report = run
        .certification("evicted proof")
        .check_fact_expect(
            "retained semantics",
            run.to_typed::<Event>()
                .unwrap()
                .values()
                .iter()
                .all(|v| v.run == 1),
            true,
        )
        .report();
    let escaped = capture_observations(config(), sun_observations::Context::capture);
    let span = capture_observations(config(), || {
        observe_span!(Event { run: 1, row: 1 }).unwrap()
    });
    certification("capture completeness prerequisites")
        .check_fact_expect("eviction count", run.status().retention.evicted, 30u64)
        .check_fact_expect(
            "failed despite valid retained records",
            report.failed,
            1usize,
        )
        .check_fact_expect(
            "classification",
            report.failures[0].class,
            CheckClass::EvaluationError,
        )
        .check_fact_expect("bounded diagnostics", report.to_string().len() < 4096, true)
        .check_fact_expect(
            "escaped context",
            escaped.certification("escape").report().is_pass(),
            false,
        )
        .check_fact_expect(
            "escaped span",
            span.certification("span").report().is_pass(),
            false,
        )
        .assert_certify();
}
#[test]
fn concurrent_completion_cannot_hide_eviction_at_the_capture_boundary() {
    let mut c = certification("concurrent capture finalization");
    for _ in 0..128 {
        let barrier = Arc::new(Barrier::new(2));
        let mut worker = None;
        let run = capture_observations(
            config().retention(MemorySinkConfig::new(1, 1.0).unwrap()),
            || {
                let ready = barrier.clone();
                worker = Some(sun_observations::context::thread::spawn(move || {
                    ready.wait();
                    execute(1, 64).unwrap();
                }));
                barrier.wait();
                std::thread::yield_now();
            },
        );
        worker.unwrap().join().unwrap();
        let report = run.certification("racing completion").report();
        c.check_fact_expect("incomplete evidence never passes", report.is_pass(), false);
        c.check_fact_expect(
            "evaluation error",
            report.failures.first().map(|failure| failure.class),
            Some(CheckClass::EvaluationError),
        );
        c.check_fact_expect("bounded snapshot", run.dataset().records().len() <= 1, true);
    }
    c.assert_certify();
}
#[test]
fn serialization_failure_is_charged_to_the_frozen_admission_context() {
    struct Switch(std::rc::Rc<std::cell::RefCell<Option<sun_observations::ContextGuard>>>);
    impl serde::Serialize for Switch {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            *self.0.borrow_mut() = Some(sun_observations::Context::default().enter());
            Err(serde::ser::Error::custom(
                "context changed during serialization",
            ))
        }
    }
    impl Observation for Switch {
        const NAMESPACE: &'static str = "capture::switch";
    }
    let guard = std::rc::Rc::new(std::cell::RefCell::new(None));
    let run = capture_observations(CaptureConfig::new().activate_all(), || {
        let _ = observe!(Switch(guard.clone()));
        drop(guard.borrow_mut().take());
    });
    certification("frozen emission health")
        .check_fact_expect(
            "original capture failure",
            run.status().emission_errors,
            1u64,
        )
        .check_fact_expect(
            "proof blocked",
            run.certification("failed").report().is_pass(),
            false,
        )
        .assert_certify();
}
#[test]
fn ignored_serialization_failure_still_blocks_capture_proof() {
    struct Broken;
    impl serde::Serialize for Broken {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("broken payload"))
        }
    }
    impl Observation for Broken {
        const NAMESPACE: &'static str = "capture::broken";
    }
    let run = capture_observations(CaptureConfig::new().activate_all(), || {
        let _ = observe!(Broken);
    });
    certification("capture error accounting")
        .check_fact_expect("emission failure", run.status().emission_errors, 1u64)
        .check_fact_expect(
            "cannot prove completeness",
            run.certification("broken").report().failures[0].class,
            CheckClass::EvaluationError,
        )
        .assert_certify();
}
