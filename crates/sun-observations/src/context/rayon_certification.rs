use crate::context::rayon::ContextThreadPool;
use crate::*;
use ::rayon::prelude::*;
use serde::Serialize;
use std::sync::Arc;
#[derive(Serialize, Observation)]
#[observation(namespace = "pools::event")]
struct Event;
#[test]
fn independent_pools_keep_construction_context_and_exact_nested_parallel_budgets() {
    let mut certification = sun_observations::certifications::certify(
        "independent pools keep construction context and exact nested parallel budgets",
    );
    let engine = Arc::new(ObservationEngine::new());
    let memory = Arc::new(MemorySink::new());
    engine
        .register_namespace_channel("pools", "memory")
        .unwrap();
    engine.add_channel_sink("memory", memory.clone());
    let make = |name| {
        let _scope = start_scope(
            ScopeConfig::new(name)
                .activate_all()
                .override_policy::<Event>(Policy::MaxPerScope(333)),
        );
        ContextThreadPool::new(4).unwrap()
    };
    let a = make("pool-a");
    let b = make("pool-b");
    let empty = ContextThreadPool::new(2).unwrap();
    let _caller = start_scope(ScopeConfig::new("caller").activate_all());
    let pools_started = std::sync::Barrier::new(2);
    std::thread::scope(|threads| {
        for pool in [&a, &b] {
            let engine = engine.clone();
            let pools_started = &pools_started;
            threads.spawn(move || {
                pool.install(|| {
                    pools_started.wait();
                    (0..32).into_par_iter().for_each(|_| {
                        (0..64).into_par_iter().for_each(|_| {
                            engine.observe_if_active(|| Event).unwrap();
                        })
                    })
                })
            });
        }
    });
    empty.install(|| {
        certification.check_fact_expect(
            "Context::capture().scope_name()",
            &(Context::capture().scope_name()),
            &(None),
        );
        certification.check_fact_expect(
            "engine .observe_if_active::<Event, _>(|| panic!(\"empty pool\")) .unwrap()",
            engine
                .observe_if_active::<Event, _>(|| panic!("empty pool"))
                .unwrap(),
            false,
        );
    });
    let worker_scopes = a.install(|| {
        (0..32)
            .into_par_iter()
            .map(|i| {
                let local = if i == 0 {
                    let _local = start_scope(ScopeConfig::new("worker-local"));
                    Context::capture().scope_name().map(str::to_owned)
                } else {
                    None
                };
                (local, Context::capture().scope_name().map(str::to_owned))
            })
            .collect::<Vec<_>>()
    });
    certification.section("worker-local restoration", |c| {
        for (i, (local, restored)) in worker_scopes.iter().enumerate() {
            if i == 0 {
                c.check_fact_expect("local override", local.as_deref(), Some("worker-local"));
            }
            c.check_fact_expect(
                format!("worker {i} restored"),
                restored.as_deref(),
                Some("pool-a"),
            );
        }
    });
    let barrier = std::sync::Barrier::new(2);
    let (_, peer_scope) = a.install(|| {
        ::rayon::join(
            || {
                let _local = start_scope(ScopeConfig::new("only-this-worker"));
                barrier.wait();
                barrier.wait();
            },
            || {
                barrier.wait();
                let scope = Context::capture().scope_name().map(str::to_owned);
                barrier.wait();
                scope
            },
        )
    });
    certification.check_fact_expect(
        "peer worker isolation",
        peer_scope.as_deref(),
        Some("pool-a"),
    );
    drop(a);
    drop(b);
    drop(empty);
    certification.check_fact_expect(
        "Context::capture().scope_name()",
        &(Context::capture().scope_name()),
        &(Some("caller")),
    );
    let records = memory.records();
    certification.check_fact_expect("records.len()", &(records.len()), &(666));
    for name in ["pool-a", "pool-b"] {
        certification.check_fact_expect(
            "records .iter() .filter(|r| r.scope.as_deref() == Some(name)) .count()",
            &(records
                .iter()
                .filter(|r| r.scope.as_deref() == Some(name))
                .count()),
            &(333),
        );
    }
    certification.assert_certify();
}

mod propagation {
    use super::super::*;
    use crate::{
        global, observe, start_scope, test_support::capture, Observation, Policy, ScopeConfig,
    };
    use ::rayon::prelude::*;
    use serde::Serialize;
    use std::sync::{Arc, Barrier};

    #[derive(Serialize, Observation)]
    #[observation(namespace = "rayon_tests::parallel")]
    struct Event {
        parent: String,
        item: usize,
    }

    #[test]
    fn dedicated_pools_inherit_each_parent_for_nested_parallel_iterations() {
        let mut certification = sun_observations::certifications::certify(
            "dedicated pools inherit each parent for nested parallel iterations",
        );
        let sink = capture(global(), "rayon_tests::parallel");
        let barrier = Arc::new(Barrier::new(2));
        let parents: Vec<_> = ["pool-a", "pool-b"]
            .into_iter()
            .map(|name| {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let scope = start_scope(
                        ScopeConfig::new(name).activate_namespace("rayon_tests::parallel"),
                    );
                    let pool = ContextThreadPool::new(3).unwrap();
                    drop(scope);
                    barrier.wait();
                    let workers = pool.install(|| {
                        let workers = ::rayon::broadcast(|_| {
                            Context::capture().scope_name().map(str::to_owned)
                        });
                        (0..8).into_par_iter().for_each(|outer| {
                            (0..16).into_par_iter().for_each(|inner| {
                                observe!(Event {
                                    parent: name.into(),
                                    item: outer * 16 + inner
                                })
                                .unwrap();
                            });
                        });
                        workers
                    });
                    (
                        name,
                        workers,
                        Context::capture().scope_name().map(str::to_owned),
                    )
                })
            })
            .collect();
        for parent in parents {
            let (name, workers, restored) = parent.join().unwrap();
            certification.section(name, |c| {
                c.check_fact_expect(
                    "construction context on all workers",
                    workers,
                    vec![Some(name.to_owned()); 3],
                );
                c.check_fact_expect("parent restored", restored, None);
            });
        }
        let records = sink.records();
        certification.check_fact_expect("records.len()", &(records.len()), &(256));
        for record in records {
            certification.check_fact_expect(
                "record.scope.as_deref()",
                &(record.scope.as_deref()),
                &(record.data["parent"].as_str()),
            );
        }
        certification.assert_certify();
    }

    #[derive(Serialize, Observation)]
    #[observation(namespace = "rayon_tests::limit")]
    struct Limited;

    #[test]
    fn pool_workers_share_policy_state_and_pool_binding_is_immutable() {
        let mut certification = sun_observations::certifications::certify(
            "pool workers share policy state and pool binding is immutable",
        );
        let sink = capture(global(), "rayon_tests::limit");
        let scope = start_scope(
            ScopeConfig::new("bound")
                .activate_namespace("rayon_tests::limit")
                .override_policy::<Limited>(Policy::MaxPerScope(17)),
        );
        let pool = ContextThreadPool::new(4).unwrap();
        drop(scope);
        let _other = start_scope(ScopeConfig::new("caller"));
        pool.install(|| {
            (0..100).into_par_iter().for_each(|_| {
                observe!(Limited).unwrap();
            })
        });
        let records = sink.records();
        certification.check_fact_expect("records.len()", &(records.len()), &(17));
        certification.check_fact_expect(
            "records.iter().all(|r| r.scope.as_deref() == Some(\"bound\"))",
            records.iter().all(|r| r.scope.as_deref() == Some("bound")),
            true,
        );
        certification.check_fact_expect(
            "Context::capture().scope_name()",
            &(Context::capture().scope_name()),
            &(Some("caller")),
        );
        certification.assert_certify();
    }

    #[test]
    fn empty_pool_context_masks_callers_scope() {
        let mut certification =
            sun_observations::certifications::certify("empty pool context masks callers scope");
        let pool = ContextThreadPool::new(2).unwrap();
        let _scope = start_scope(ScopeConfig::new("caller"));
        let scopes = pool.install(|| {
            (0..30)
                .into_par_iter()
                .map(|_| Context::capture().scope_name().map(str::to_owned))
                .collect::<Vec<_>>()
        });
        certification.check_fact_expect(
            "empty construction context on every worker",
            scopes,
            vec![None; 30],
        );
        certification.check_fact_expect(
            "Context::capture().scope_name()",
            &(Context::capture().scope_name()),
            &(Some("caller")),
        );
        certification.assert_certify();
    }
}
