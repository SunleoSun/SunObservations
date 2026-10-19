use super::*;
use crate::{
    global, observe, start_scope, support_test::capture, Observation, Policy, ScopeConfig,
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
    assert_eq!(records.len(), 82);
    for record in records {
        assert_eq!(record.scope.as_deref(), record.data["parent"].as_str());
    }
    assert_eq!(Context::capture().scope_name(), None);
}

#[derive(Serialize, Observation)]
#[observation(namespace = "thread_tests::policies")]
struct Limited;

#[test]
fn inherited_threads_share_parent_counter_but_not_other_scopes() {
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
    assert_eq!(records.len(), 26);
    for name in ["a", "b"] {
        assert_eq!(
            records
                .iter()
                .filter(|r| r.scope.as_deref() == Some(name))
                .count(),
            13
        );
    }
}

#[test]
fn plain_std_spawn_does_not_inherit_tls() {
    let _scope = start_scope(ScopeConfig::new("parent"));
    assert_eq!(
        std::thread::spawn(|| Context::capture().scope_name().map(str::to_owned))
            .join()
            .unwrap(),
        None
    );
    assert_eq!(
        spawn(|| Context::capture().scope_name().map(str::to_owned))
            .join()
            .unwrap()
            .as_deref(),
        Some("parent")
    );
}
