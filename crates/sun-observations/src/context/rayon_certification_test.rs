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
        assert_eq!(Context::capture().scope_name(), None);
        assert!(!engine
            .observe_if_active::<Event, _>(|| panic!("empty pool"))
            .unwrap());
    });
    a.install(|| {
        (0..32).into_par_iter().for_each(|i| {
            if i == 0 {
                let _local = start_scope(ScopeConfig::new("worker-local"));
                assert_eq!(Context::capture().scope_name(), Some("worker-local"));
            }
            assert_eq!(Context::capture().scope_name(), Some("pool-a"));
        });
    });
    let barrier = std::sync::Barrier::new(2);
    a.install(|| {
        ::rayon::join(
            || {
                let _local = start_scope(ScopeConfig::new("only-this-worker"));
                barrier.wait();
                barrier.wait();
            },
            || {
                barrier.wait();
                assert_eq!(Context::capture().scope_name(), Some("pool-a"));
                barrier.wait();
            },
        )
    });
    drop(a);
    drop(b);
    drop(empty);
    assert_eq!(Context::capture().scope_name(), Some("caller"));
    let records = memory.records();
    assert_eq!(records.len(), 666);
    for name in ["pool-a", "pool-b"] {
        assert_eq!(
            records
                .iter()
                .filter(|r| r.scope.as_deref() == Some(name))
                .count(),
            333
        );
    }
}
