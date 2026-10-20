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
    let engine = Arc::new(ObservationEngine::new());
    let memory = Arc::new(MemorySink::new());
    engine.add_channel_sink("memory", memory.clone());
    engine
        .register_namespace_channel("threads", "memory")
        .unwrap();
    let barrier = Arc::new(Barrier::new(6));
    std::thread::scope(|parents| {
        for parent in 0..6 {
            let engine = engine.clone();
            let barrier = barrier.clone();
            parents.spawn(move || {
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
                        assert!(std::panic::catch_unwind(
                            || Context::default().run(|| panic!("child unwind"))
                        )
                        .is_err());
                        assert_eq!(
                            Context::capture().scope_name(),
                            Some(format!("parent-{parent}").as_str())
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
                    }));
                }
                for child in children {
                    child.join().unwrap();
                }
                let engine_plain = engine.clone();
                assert!(!std::thread::spawn(
                    move || engine_plain.observe_if_active::<Event, _>(|| panic!("plain thread"))
                )
                .join()
                .unwrap()
                .unwrap());
                assert!(context::thread::spawn(|| {
                    let _inner = start_scope(ScopeConfig::new("panic"));
                    panic!("unwind");
                })
                .join()
                .is_err());
                assert_eq!(Context::capture().scope_name(), Some(name.as_str()));
                assert!(!engine
                    .observe_if_active::<Event, _>(|| panic!("shared cap"))
                    .unwrap());
            });
        }
    });
    let records = memory.records();
    assert_eq!(records.len(), 6 * 237);
    for parent in 0..6 {
        let own: Vec<_> = records
            .iter()
            .filter(|r| r.data["parent"] == parent)
            .collect();
        assert_eq!(own.len(), 237);
        assert!(own
            .iter()
            .all(|r| r.scope.as_deref() == Some(format!("parent-{parent}").as_str())));
    }
}
