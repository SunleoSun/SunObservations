use crate::*;
use serde::Serialize;
use std::sync::{Arc, Barrier};
#[derive(Serialize, Observation)]
#[observation(namespace = "threads::event")]
struct Event {
    parent: usize,
}
#[test]
fn simultaneous_parents_transitive_children_unwind_and_plain_threads_are_isolated() {
    let mut certification =
        sun_certifications::certify("transitive thread context isolation and unwind");
    let engine = Arc::new(ObservationEngine::new());
    let memory = Arc::new(MemorySink::new());
    engine.add_channel_sink("memory", memory.clone());
    engine
        .register_namespace_channel("threads", "memory")
        .unwrap();
    let barrier = Arc::new(Barrier::new(6));
    let parent_reports = std::thread::scope(|parents| {
        let mut workers = Vec::new();
        for parent in 0..6 {
            let engine = engine.clone();
            let barrier = barrier.clone();
            workers.push(parents.spawn(move || {
                let mut certification = sun_certifications::certify(format!("parent {parent}"));
                let name = format!("parent-{parent}");
                let _scope = start_scope(
                    ScopeConfig::new(&name)
                        .activate_all()
                        .override_policy::<Event>(Policy::FirstN(237)),
                );
                barrier.wait();
                let mut children = Vec::new();
                for _ in 0..5 {
                    let engine = engine.clone();
                    children.push(context::thread::spawn(move || {
                        let mut certification =
                            sun_certifications::certify("transitive child context");
                        certification.check_fact_expect(
                            "child unwind propagates",
                            std::panic::catch_unwind(|| {
                                Context::default().run(|| panic!("child unwind"))
                            })
                            .is_err(),
                            true,
                        );
                        certification.check_fact_expect(
                            "Context::capture().scope_name()",
                            &(Context::capture().scope_name()),
                            &(Some(format!("parent-{parent}").as_str())),
                        );
                        let nested_engine = engine.clone();
                        context::thread::spawn(move || {
                            for _ in 0..100 {
                                nested_engine
                                    .observe_if_active(|| Event { parent })
                                    .unwrap();
                            }
                        })
                        .join()
                        .unwrap();
                        for _ in 0..100 {
                            engine.observe_if_active(|| Event { parent }).unwrap();
                        }
                        certification.certify()
                    }));
                }
                for child in children {
                    let report = child.join().unwrap();
                    certification.check(
                        "transitive child",
                        sun_certifications::fact("child", report.is_pass())
                            .details(report.to_string())
                            .expect(true),
                    );
                }
                let engine_plain = engine.clone();
                certification.check_fact_expect(
                    "plain thread does not inherit admission",
                    std::thread::spawn(move || {
                        engine_plain.observe_if_active::<Event, _>(|| panic!("plain thread"))
                    })
                    .join()
                    .unwrap()
                    .unwrap(),
                    false,
                );
                certification.check_fact_expect(
                    "propagated child panic reaches join",
                    context::thread::spawn(|| {
                        let _inner = start_scope(ScopeConfig::new("panic"));
                        panic!("unwind");
                    })
                    .join()
                    .is_err(),
                    true,
                );
                certification.check_fact_expect(
                    "Context::capture().scope_name()",
                    &(Context::capture().scope_name()),
                    &(Some(name.as_str())),
                );
                certification.check_fact_expect(
                    "shared parent budget exhausted",
                    engine
                        .observe_if_active::<Event, _>(|| panic!("shared cap"))
                        .unwrap(),
                    false,
                );
                certification.certify()
            }));
        }
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    for report in parent_reports {
        certification.check(
            "parent context",
            sun_certifications::fact("parent", report.is_pass())
                .details(report.to_string())
                .expect(true),
        );
    }
    let records = memory.records();
    certification.check_fact_expect("records.len()", &(records.len()), &(6 * 237));
    for parent in 0..6 {
        let own: Vec<_> = records
            .iter()
            .filter(|r| r.data["parent"] == parent)
            .collect();
        certification.check_fact_expect("own.len()", &(own.len()), &(237));
        certification.check_fact_expect(
            "all records belong to their parent scope",
            own.iter()
                .all(|r| r.scope.as_deref() == Some(format!("parent-{parent}").as_str())),
            true,
        );
    }
    certification.assert_certify();
}

mod propagation {
    use super::super::*;
    use crate::{
        global, observe, start_scope, test_support::capture, Observation, Policy, ScopeConfig,
    };
    use serde::Serialize;
    use std::sync::{Arc, Barrier};

    #[derive(Serialize, Observation)]
    #[observation(namespace = "thread_tests::inherited")]
    struct Event {
        parent: String,
    }

    #[test]
    fn two_concurrent_parents_and_children_do_not_mix_scopes() {
        let mut certification =
            sun_certifications::certify("two concurrent parents and children do not mix scopes");
        let sink = capture(global(), "thread_tests::inherited");
        let barrier = Arc::new(Barrier::new(2));
        let parents: Vec<_> = ["parent-a", "parent-b"]
            .into_iter()
            .map(|name| {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let _scope = start_scope(
                        ScopeConfig::new(name).activate_namespace("thread_tests::inherited"),
                    );
                    barrier.wait();
                    spawn(move || {
                        for _ in 0..40 {
                            observe!(Event {
                                parent: name.into()
                            })
                            .unwrap();
                        }
                    })
                    .join()
                    .unwrap();
                    observe!(Event {
                        parent: name.into()
                    })
                    .unwrap();
                })
            })
            .collect();
        for parent in parents {
            parent.join().unwrap();
        }
        let records = sink.records();
        certification.check_fact_expect("records.len()", &(records.len()), &(82));
        for record in records {
            certification.check_fact_expect(
                "record.scope.as_deref()",
                &(record.scope.as_deref()),
                &(record.data["parent"].as_str()),
            );
        }
        certification.check_fact_expect(
            "Context::capture().scope_name()",
            &(Context::capture().scope_name()),
            &(None),
        );
        certification.assert_certify();
    }

    #[derive(Serialize, Observation)]
    #[observation(namespace = "thread_tests::policies")]
    struct Limited;

    #[test]
    fn inherited_threads_share_parent_counter_but_not_other_scopes() {
        let mut certification = sun_certifications::certify(
            "inherited threads share parent counter but not other scopes",
        );
        let sink = capture(global(), "thread_tests::policies");
        for name in ["a", "b"] {
            let _scope = start_scope(
                ScopeConfig::new(name)
                    .activate_namespace("thread_tests::policies")
                    .override_policy::<Limited>(Policy::FirstN(13)),
            );
            let children: Vec<_> = (0..8)
                .map(|_| {
                    spawn(|| {
                        for _ in 0..20 {
                            observe!(Limited).unwrap();
                        }
                    })
                })
                .collect();
            for child in children {
                child.join().unwrap();
            }
        }
        let records = sink.records();
        certification.check_fact_expect("records.len()", &(records.len()), &(26));
        for name in ["a", "b"] {
            certification.check_fact_expect(
                "records .iter() .filter(|r| r.scope.as_deref() == Some(name)) .count()",
                &(records
                    .iter()
                    .filter(|r| r.scope.as_deref() == Some(name))
                    .count()),
                &(13),
            );
        }
        certification.assert_certify();
    }

    #[test]
    fn plain_std_spawn_does_not_inherit_tls() {
        let mut certification = sun_certifications::certify("plain std spawn does not inherit tls");
        let _scope = start_scope(ScopeConfig::new("parent"));
        certification.check_fact_expect(
            "ordinary thread has no inherited context",
            &(std::thread::spawn(|| Context::capture().scope_name().map(str::to_owned))
                .join()
                .unwrap()),
            &(None),
        );
        certification.check_fact_expect(
            "propagated thread retains parent context",
            &(spawn(|| Context::capture().scope_name().map(str::to_owned))
                .join()
                .unwrap()
                .as_deref()),
            &(Some("parent")),
        );
        certification.assert_certify();
    }
}
