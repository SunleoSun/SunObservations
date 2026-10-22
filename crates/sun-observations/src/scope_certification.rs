use crate::*;
use serde::Serialize;
use std::sync::Arc;
#[derive(Serialize, Observation)]
#[observation(namespace = "budget::event")]
struct Event;
#[test]
fn concurrent_scope_budgets_are_exact_independent_and_survive_guard_drop() {
    let mut certification = sun_observations::certifications::certification(
        "concurrent scope budgets are exact independent and survive guard drop",
    );
    let engine = Arc::new(ObservationEngine::isolated());
    engine.set_selection(Selection::new().activate_all());
    for (policy, expected) in [
        (Policy::Always, 8000),
        (Policy::Never, 0),
        (Policy::first_n(337).unwrap(), 337),
        (Policy::every_n(7).unwrap(), 8000 / 7),
        (Policy::first_n(517).unwrap(), 517),
    ] {
        for _ in 0..2 {
            let scope = start_scope(ScopeConfig::new("same-name").override_policy::<Event>(policy));
            let context = Context::capture();
            drop(scope);
            let admitted = std::thread::scope(|threads| {
                let mut handles = Vec::new();
                for _ in 0..8 {
                    let engine = engine.clone();
                    let context = context.clone();
                    handles.push(threads.spawn(move || {
                        context.run(|| {
                            (0..1000)
                                .filter(|_| engine.observe_if_active(|| Event).unwrap())
                                .count()
                        })
                    }));
                }
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap())
                    .sum::<usize>()
            });
            certification.check_fact_expect("admitted", &(admitted), &(expected));
        }
    }
    certification.check_fact_expect(
        "engine.observe_if_active(|| Event).unwrap()",
        engine.observe_if_active(|| Event).unwrap(),
        true,
    );
    certification.assert_certify();
}
#[test]
fn nested_selection_and_policy_are_fresh_while_guards_restore_or_mask_execution() {
    let mut certification = sun_observations::certifications::certification(
        "nested selection and policy are fresh while guards restore or mask execution",
    );
    #[derive(Serialize, Observation)]
    #[observation(namespace = "budget::other")]
    struct Other;
    let engine = ObservationEngine::isolated();
    let outer = start_scope(
        ScopeConfig::new("outer")
            .activate_all()
            .override_policy::<Event>(Policy::Never),
    );
    certification.check_fact_expect(
        "engine.observe_if_active(|| Other).unwrap()",
        engine.observe_if_active(|| Other).unwrap(),
        true,
    );
    {
        let _empty = start_scope(ScopeConfig::new("empty"));
        certification.check_fact_expect(
            "inner scope does not inherit selection",
            engine
                .observe_if_active::<Other, _>(|| panic!("no inherited selection"))
                .unwrap(),
            false,
        );
    }
    certification.check_fact_expect(
        "engine .observe_if_active::<Event, _>(|| panic!(\"outer override\")) .unwrap()",
        engine
            .observe_if_active::<Event, _>(|| panic!("outer override"))
            .unwrap(),
        false,
    );
    let inner = start_scope(ScopeConfig::new("inner").activate_all());
    certification.check_fact_expect(
        "engine.observe_if_active(|| Event).unwrap()",
        engine.observe_if_active(|| Event).unwrap(),
        true,
    );
    Context::default().run(|| {
        certification.check_fact_expect(
            "engine .observe_if_active::<Event, _>(|| panic!(\"masked\")) .unwrap()",
            engine
                .observe_if_active::<Event, _>(|| panic!("masked"))
                .unwrap(),
            false,
        );
    });
    certification.check_fact_expect(
        "Context::capture().scope_name()",
        &(Context::capture().scope_name()),
        &(Some("inner")),
    );
    let captured = Context::capture();
    drop(outer);
    drop(inner);
    certification.check_fact_expect(
        "Context::capture().scope_name()",
        &(Context::capture().scope_name()),
        &(None),
    );
    certification.check_fact_expect(
        "engine .observe_if_active::<Event, _>(|| panic!(\"not resurrected\")) .unwrap()",
        engine
            .observe_if_active::<Event, _>(|| panic!("not resurrected"))
            .unwrap(),
        false,
    );
    captured.run(|| {
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event).unwrap()",
            engine.observe_if_active(|| Event).unwrap(),
            true,
        );
    });
    certification.check_fact_expect(
        "Context::capture().scope_name()",
        &(Context::capture().scope_name()),
        &(None),
    );
    certification.assert_certify();
}

#[test]
fn saturated_counter_never_wraps_under_concurrent_admission() {
    let mut certification = sun_observations::certifications::certification(
        "saturated counter never wraps under concurrent admission",
    );
    use std::sync::atomic::{AtomicU64, Ordering};
    let counter = AtomicU64::new(u64::MAX - 2);
    let rejections = std::thread::scope(|threads| {
        let mut workers = Vec::new();
        for _ in 0..8 {
            let counter = &counter;
            workers.push(threads.spawn(move || {
                (0..1000)
                    .filter(|_| !Policy::first_n(10).unwrap().admit_atomic(counter))
                    .count()
            }));
        }
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .sum::<usize>()
    });
    certification.check_fact_expect("all saturated attempts rejected", rejections, 8000usize);
    certification.check_fact_expect(
        "counter.load(Ordering::Relaxed)",
        &(counter.load(Ordering::Relaxed)),
        &(u64::MAX),
    );
    certification.assert_certify();
}

mod lifecycle {
    use super::super::*;
    use crate::{test_support::capture, ObservationEngine};
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
        let mut certification = sun_observations::certifications::certification(
            "counters are isolated between scopes even with equal names",
        );
        let engine = ObservationEngine::isolated();
        let sink = capture(&engine, "scope_tests");
        let config = ScopeConfig::new("same-name")
            .activate_namespace("scope_tests")
            .unwrap()
            .override_policy::<Event>(Policy::every_n(2).unwrap());
        for _ in 0..2 {
            let _scope = start_scope(config.clone());
            certification.check_fact_expect(
                "engine.observe_if_active(|| Event { value: 1 }).unwrap()",
                engine.observe_if_active(|| Event { value: 1 }).unwrap(),
                false,
            );
            certification.check_fact_expect(
                "engine.observe_if_active(|| Event { value: 2 }).unwrap()",
                engine.observe_if_active(|| Event { value: 2 }).unwrap(),
                true,
            );
            certification.check_fact_expect(
                "engine.observe_if_active(|| Other).unwrap()",
                engine.observe_if_active(|| Other).unwrap(),
                true,
            );
        }
        certification.check_fact_expect("sink.len()", &(sink.len()), &(4));
        certification.assert_certify();
    }

    #[test]
    fn first_n_and_max_per_scope_reset_and_overrides_do_not_escape() {
        let mut certification = sun_observations::certifications::certification(
            "first n and max per scope reset and overrides do not escape",
        );
        let engine = ObservationEngine::isolated();
        let sink = capture(&engine, "scope_tests");
        engine.set_selection(Selection::new().activate_namespace("scope_tests").unwrap());
        for policy in [
            Policy::first_n(1).unwrap(),
            Policy::first_n(1).unwrap(),
            Policy::Never,
        ] {
            for name in ["a", "b"] {
                let _scope = start_scope(ScopeConfig::new(name).override_policy::<Event>(policy));
                certification.check_fact_expect(
                    "engine.observe_if_active(|| Event { value: 1 }).unwrap()",
                    &(engine.observe_if_active(|| Event { value: 1 }).unwrap()),
                    &(policy != Policy::Never),
                );
                certification.check_fact_expect(
                    "engine.observe_if_active(|| Event { value: 2 }).unwrap()",
                    engine.observe_if_active(|| Event { value: 2 }).unwrap(),
                    false,
                );
            }
        }
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event { value: 3 }).unwrap()",
            engine.observe_if_active(|| Event { value: 3 }).unwrap(),
            true,
        );
        certification.check_fact_expect("sink.len()", &(sink.len()), &(5));
        certification.check_fact_expect(
            "sink.records().last().unwrap().scope",
            &(sink.records().last().unwrap().scope),
            &(None),
        );
        certification.assert_certify();
    }

    #[test]
    fn filtered_occurrences_do_not_advance_policy() {
        let mut certification = sun_observations::certifications::certification(
            "filtered occurrences do not advance policy",
        );
        let engine = ObservationEngine::isolated();
        let _scope = start_scope(
            ScopeConfig::new("policy").override_policy::<Event>(Policy::every_n(2).unwrap()),
        );
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event { value: 1 }).unwrap()",
            engine.observe_if_active(|| Event { value: 1 }).unwrap(),
            false,
        );
        engine.set_selection(Selection::new().activate_namespace("scope_tests").unwrap());
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event { value: 2 }).unwrap()",
            engine.observe_if_active(|| Event { value: 2 }).unwrap(),
            false,
        );
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event { value: 3 }).unwrap()",
            engine.observe_if_active(|| Event { value: 3 }).unwrap(),
            true,
        );
        certification.assert_certify();
    }
}
