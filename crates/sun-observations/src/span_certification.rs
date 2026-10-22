use crate::*;
use serde::Serialize;
use std::{collections::HashMap, io, sync::Arc};
#[derive(Serialize)]
struct Work {
    id: usize,
}
impl Observation for Work {
    const NAMESPACE: &'static str = "spans::work";
    const LEVEL: ObservationLevel = ObservationLevel::WARN;
    fn message(&self) -> Option<&str> {
        Some("typed start")
    }
}
#[test]
fn concurrent_pairs_freeze_scope_routing_and_budget_through_mutation_and_unwind() {
    let mut certification = sun_observations::certifications::certify(
        "concurrent pairs freeze scope routing and budget through mutation and unwind",
    );
    let engine = ObservationEngine::new();
    let old = Arc::new(MemorySink::new());
    let new = Arc::new(MemorySink::new());
    engine.register_namespace_channel("spans", "route").unwrap();
    engine.add_channel_sink("route", old.clone());
    let scope = start_scope(
        ScopeConfig::new("origin")
            .activate_all()
            .override_policy::<Work>(Policy::FirstN(101)),
    );
    let context = Context::capture();
    let active = std::thread::scope(|threads| {
        let mut workers = Vec::new();
        for id in 0..100 {
            let context = context.clone();
            let engine = &engine;
            workers.push(threads.spawn(move || {
                context.run(|| {
                    let span = engine.span_if_active(|| Work { id }).unwrap();
                    let active = span.is_active();
                    span.finish().unwrap();
                    active
                })
            }));
        }
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    certification.check_fact_expect("all concurrent spans admitted", active, vec![true; 100]);
    let last = engine.span_if_active(|| Work { id: 100 }).unwrap();
    certification.check_fact_expect(
        "engine .span_if_active::<Work, _>(|| panic!(\"count once\")) .unwrap() .is_active()",
        engine
            .span_if_active::<Work, _>(|| panic!("count once"))
            .unwrap()
            .is_active(),
        false,
    );
    engine.clear_channel_sinks("route");
    engine.add_channel_sink("route", new.clone());
    engine.set_selection(Selection::new().exclude_namespace("spans"));
    drop(scope);
    last.finish().unwrap();
    certification.check_fact_expect("new.is_empty()", new.is_empty(), true);
    let records = old.records();
    certification.check_fact_expect("records.len()", &(records.len()), &(202));
    let mut pairs = HashMap::<_, Vec<_>>::new();
    for r in &records {
        pairs.entry(r.span.clone().unwrap()).or_default().push(r);
    }
    certification.check_fact_expect("pairs.len()", &(pairs.len()), &(101));
    for pair in pairs.values() {
        certification.check_fact_expect("pair.len()", &(pair.len()), &(2));
        let (start, end) = (pair[0], pair[1]);
        certification.check_fact_expect("start.phase", &(start.phase), &(Some(Phase::Start)));
        certification.check_fact_expect("end.phase", &(end.phase), &(Some(Phase::End)));
        certification.check_fact_expect(
            "(&start.proc, &start.scope, &start.namespace, start.level)",
            &(&start.proc, &start.scope, &start.namespace, start.level),
            &(&end.proc, &end.scope, &end.namespace, end.level),
        );
        certification.check_fact_expect(
            "start.scope.as_deref()",
            &(start.scope.as_deref()),
            &(Some("origin")),
        );
        certification.check_fact_expect(
            "start.data[\"id\"].is_u64()",
            start.data["id"].is_u64(),
            true,
        );
        certification.check_fact_expect(
            "start.message.as_deref()",
            &(start.message.as_deref()),
            &(Some("typed start")),
        );
        certification.check_fact_expect("end.data.is_null()", end.data.is_null(), true);
        certification.check_fact_expect("end.message.is_none()", end.message.is_none(), true);
    }
    engine.set_selection(Selection::new().activate_all());
    certification.check_fact_expect(
        "concurrent pairs freeze scope routing and budget through mutation and unwind",
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _span = engine.span_if_active(|| Work { id: 200 }).unwrap();
            panic!("unwind");
        }))
        .is_err(),
        true,
    );
    certification.check_fact_expect("new.len()", &(new.len()), &(2));
    certification.assert_certify();
}
struct EndFailure;
impl Sink for EndFailure {
    fn write(&self, record: &ObservationRecord) -> io::Result<()> {
        if record.phase == Some(Phase::End) {
            Err(io::Error::other("end"))
        } else {
            Ok(())
        }
    }
}
#[test]
fn explicit_end_failure_consumes_one_end_and_one_checked_error() {
    let mut certification = sun_observations::certifications::certify(
        "explicit end failure consumes one end and one checked error",
    );
    let engine = ObservationEngine::new();
    let memory = Arc::new(MemorySink::new());
    engine.set_selection(Selection::new().activate_all());
    engine.register_namespace_channel("spans", "route").unwrap();
    engine.add_channel_sink("route", Arc::new(EndFailure));
    engine.add_channel_sink("route", memory.clone());
    certification.check_fact_expect(
        "engine .span_if_active(|| Work { id: 0 }) .unwrap() .finish() .is_err()",
        engine
            .span_if_active(|| Work { id: 0 })
            .unwrap()
            .finish()
            .is_err(),
        true,
    );
    certification.check_fact_expect(
        "engine.sink_error_count()",
        &(engine.sink_error_count()),
        &(1),
    );
    certification.check_fact_expect("memory.len()", &(memory.len()), &(2));
    certification.assert_certify();
}

mod lifecycle {
    use crate::{
        global, observe_span, start_scope, test_support::capture, Observation, ObservationEngine,
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
        let mut certification = sun_observations::certifications::certify(
            "start end share identity scope and frozen routing",
        );
        let engine = ObservationEngine::new();
        let sink = capture(&engine, "span_tests");
        let scope = start_scope(ScopeConfig::new("fit-run").activate_namespace("span_tests"));
        let span = engine.span_if_active(|| Fit { samples: 42 }).unwrap();
        certification.check_fact_expect("span.is_active()", span.is_active(), true);
        certification.check_fact_expect("sink.len()", &(sink.len()), &(1));
        drop(scope);
        engine.set_selection(Selection::new());
        let later = Arc::new(crate::MemorySink::new());
        engine.add_channel_sink("span_tests", later.clone());
        drop(span);
        let records = sink.records();
        certification.check_fact_expect("records.len()", &(records.len()), &(2));
        certification.check_fact_expect("later.is_empty()", later.is_empty(), true);
        let (start, end) = (&records[0], &records[1]);
        certification.check_fact_expect("start.phase", &(start.phase), &(Some(Phase::Start)));
        certification.check_fact_expect("end.phase", &(end.phase), &(Some(Phase::End)));
        certification.check_fact_expect("start.namespace", &(start.namespace), &(end.namespace));
        certification.check_fact_expect(
            "start.scope.as_deref()",
            &(start.scope.as_deref()),
            &(Some("fit-run")),
        );
        certification.check_fact_expect("start.scope", &(start.scope), &(end.scope));
        certification.check_fact_expect("start.span.is_some()", start.span.is_some(), true);
        certification.check_fact_expect("start.span", &(start.span), &(end.span));
        certification.check_fact_expect("start.proc", &(start.proc), &(end.proc));
        certification.check_fact_expect(
            "end.proc_ms >= start.proc_ms",
            end.proc_ms >= start.proc_ms,
            true,
        );
        certification.check_fact_expect(
            "start.data",
            &(start.data),
            &(serde_json::json!({"samples": 42})),
        );
        certification.check_fact_expect("end.data.is_null()", end.data.is_null(), true);
        certification.assert_certify();
    }

    #[test]
    fn rejected_spans_are_lazy_and_have_no_partial_lifecycle() {
        let mut certification = sun_observations::certifications::certify(
            "rejected spans are lazy and have no partial lifecycle",
        );
        let engine = ObservationEngine::new();
        let sink = capture(&engine, "span_tests");
        let calls = AtomicUsize::new(0);
        let build = || Fit {
            samples: calls.fetch_add(1, Ordering::Relaxed),
        };
        certification.check_fact_expect(
            "engine.span_if_active(build).unwrap().is_active()",
            engine.span_if_active(build).unwrap().is_active(),
            false,
        );
        {
            let _scope = start_scope(
                ScopeConfig::new("never")
                    .activate_namespace("span_tests")
                    .override_policy::<Fit>(Policy::Never),
            );
            certification.check_fact_expect(
                "engine.span_if_active(build).unwrap().is_active()",
                engine.span_if_active(build).unwrap().is_active(),
                false,
            );
        }
        {
            let _scope = start_scope(
                ScopeConfig::new("excluded")
                    .activate_namespace("span_tests")
                    .exclude_namespace("span_tests"),
            );
            certification.check_fact_expect(
                "engine.span_if_active(build).unwrap().is_active()",
                engine.span_if_active(build).unwrap().is_active(),
                false,
            );
        }
        certification.check_fact_expect(
            "calls.load(Ordering::Relaxed)",
            &(calls.load(Ordering::Relaxed)),
            &(0),
        );
        certification.check_fact_expect("sink.is_empty()", sink.is_empty(), true);
        certification.assert_certify();
    }

    #[test]
    fn macro_spans_have_distinct_ids_and_end_during_unwind() {
        let mut certification = sun_observations::certifications::certify(
            "macro spans have distinct ids and end during unwind",
        );
        let sink = capture(global(), "span_tests::fit");
        let _scope = start_scope(ScopeConfig::new("unwind").activate_namespace("span_tests"));
        let result = std::panic::catch_unwind(|| {
            let _a = observe_span!(Fit { samples: 1 }).unwrap();
            let _b = observe_span!(Fit { samples: 2 }).unwrap();
            panic!("unwind");
        });
        certification.check_fact_expect("result.is_err()", result.is_err(), true);
        let records = sink.records();
        certification.check_fact_expect("records.len()", &(records.len()), &(4));
        certification.check_fact_expect(
            "records[0].span",
            &(records[0].span),
            sun_observations::certifications::NotEqual(&(records[1].span)),
        );
        certification.check_fact_expect("records[0].span", &(records[0].span), &(records[3].span));
        certification.check_fact_expect("records[1].span", &(records[1].span), &(records[2].span));
        certification.check_fact_expect(
            "records[3].phase",
            &(records[3].phase),
            &(Some(Phase::End)),
        );
        certification.assert_certify();
    }

    #[test]
    fn policy_counts_one_span_not_its_lifecycle_records() {
        let mut certification = sun_observations::certifications::certify(
            "policy counts one span not its lifecycle records",
        );
        let engine = ObservationEngine::new();
        let sink = capture(&engine, "span_tests");
        let _scope = start_scope(
            ScopeConfig::new("policy")
                .activate_namespace("span_tests")
                .override_policy::<Fit>(Policy::EveryN(2)),
        );
        for accepted in [false, true, false, true] {
            let span = engine.span_if_active(|| Fit { samples: 10 }).unwrap();
            certification.check_fact_expect("span.is_active()", &(span.is_active()), &(accepted));
            span.finish().unwrap();
        }
        certification.check_fact_expect("sink.len()", &(sink.len()), &(4));
        certification.check_fact_expect(
            "sink.records() .iter() .filter(|r| r.phase == Some(Phase::End)) .count()",
            &(sink
                .records()
                .iter()
                .filter(|r| r.phase == Some(Phase::End))
                .count()),
            &(2),
        );
        certification.assert_certify();
    }

    #[test]
    fn explicit_finish_reports_end_error_and_never_retries() {
        let mut certification = sun_observations::certifications::certify(
            "explicit finish reports end error and never retries",
        );
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
        certification.check_fact_expect(
            "engine .span_if_active(|| Fit { samples: 1 }) .unwrap() .finish() .is_err()",
            engine
                .span_if_active(|| Fit { samples: 1 })
                .unwrap()
                .finish()
                .is_err(),
            true,
        );
        certification.check_fact_expect(
            "sink.0.load(Ordering::Relaxed)",
            &(sink.0.load(Ordering::Relaxed)),
            &(2),
        );
        certification.check_fact_expect(
            "engine.sink_error_count()",
            &(engine.sink_error_count()),
            &(1),
        );
        certification.assert_certify();
    }
}
