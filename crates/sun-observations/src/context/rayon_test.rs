use super::*;
use crate::{
    global, observe, start_scope, support_test::capture, Observation, Policy, ScopeConfig,
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
    let sink = capture(global(), "rayon_tests::parallel");
    let barrier = Arc::new(Barrier::new(2));
    let parents: Vec<_> = ["pool-a", "pool-b"]
        .into_iter()
        .map(|name| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let scope =
                    start_scope(ScopeConfig::new(name).activate_namespace("rayon_tests::parallel"));
                let pool = ContextThreadPool::new(3).unwrap();
                drop(scope);
                barrier.wait();
                pool.install(|| {
                    let workers =
                        ::rayon::broadcast(|_| Context::capture().scope_name().map(str::to_owned));
                    assert_eq!(workers, vec![Some(name.to_owned()); 3]);
                    (0..8).into_par_iter().for_each(|outer| {
                        (0..16).into_par_iter().for_each(|inner| {
                            observe!(Event {
                                parent: name.into(),
                                item: outer * 16 + inner
                            })
                            .unwrap();
                        });
                    })
                });
                assert_eq!(Context::capture().scope_name(), None);
            })
        })
        .collect();
    for parent in parents {
        parent.join().unwrap();
    }
    let records = sink.records();
    assert_eq!(records.len(), 256);
    for record in records {
        assert_eq!(record.scope.as_deref(), record.data["parent"].as_str());
    }
}

#[derive(Serialize, Observation)]
#[observation(namespace = "rayon_tests::limit")]
struct Limited;

#[test]
fn pool_workers_share_policy_state_and_pool_binding_is_immutable() {
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
    assert_eq!(records.len(), 17);
    assert!(records.iter().all(|r| r.scope.as_deref() == Some("bound")));
    assert_eq!(Context::capture().scope_name(), Some("caller"));
}

#[test]
fn empty_pool_context_masks_callers_scope() {
    let pool = ContextThreadPool::new(2).unwrap();
    let _scope = start_scope(ScopeConfig::new("caller"));
    pool.install(|| {
        (0..30)
            .into_par_iter()
            .for_each(|_| assert_eq!(Context::capture().scope_name(), None))
    });
    assert_eq!(Context::capture().scope_name(), Some("caller"));
}
