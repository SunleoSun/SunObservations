use crate::*;
use serde::Serialize;
use std::sync::Arc;
#[derive(Serialize, Observation)]
#[observation(namespace = "budget::event")]
struct Event;
#[test]
fn concurrent_scope_budgets_are_exact_independent_and_survive_guard_drop() {
    let engine = Arc::new(ObservationEngine::new());
    engine.set_selection(Selection::new().activate_all());
    for (policy, expected) in [
        (Policy::Always, 8000),
        (Policy::Never, 0),
        (Policy::FirstN(337), 337),
        (Policy::EveryN(7), 8000 / 7),
        (Policy::MaxPerScope(517), 517),
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
            assert_eq!(admitted, expected);
        }
    }
    assert!(engine.observe_if_active(|| Event).unwrap());
}
#[test]
fn nested_selection_and_policy_are_fresh_while_guards_restore_or_mask_execution() {
    #[derive(Serialize, Observation)]
    #[observation(namespace = "budget::other")]
    struct Other;
    let engine = ObservationEngine::new();
    let outer = start_scope(
        ScopeConfig::new("outer")
            .activate_all()
            .override_policy::<Event>(Policy::Never),
    );
    assert!(engine.observe_if_active(|| Other).unwrap());
    {
        let _empty = start_scope(ScopeConfig::new("empty"));
        assert!(!engine
            .observe_if_active::<Other, _>(|| panic!("no inherited selection"))
            .unwrap());
    }
    assert!(!engine
        .observe_if_active::<Event, _>(|| panic!("outer override"))
        .unwrap());
    let inner = start_scope(ScopeConfig::new("inner").activate_all());
    assert!(engine.observe_if_active(|| Event).unwrap());
    Context::default().run(|| {
        assert!(!engine
            .observe_if_active::<Event, _>(|| panic!("masked"))
            .unwrap())
    });
    assert_eq!(Context::capture().scope_name(), Some("inner"));
    let captured = Context::capture();
    drop(outer);
    drop(inner);
    assert_eq!(Context::capture().scope_name(), None);
    assert!(!engine
        .observe_if_active::<Event, _>(|| panic!("not resurrected"))
        .unwrap());
    captured.run(|| assert!(engine.observe_if_active(|| Event).unwrap()));
    assert_eq!(Context::capture().scope_name(), None);
}

#[test]
fn saturated_counter_never_wraps_under_concurrent_admission() {
    use std::sync::atomic::{AtomicU64, Ordering};
    let counter = AtomicU64::new(u64::MAX - 2);
    std::thread::scope(|threads| {
        for _ in 0..8 {
            let counter = &counter;
            threads.spawn(move || {
                for _ in 0..1000 {
                    assert!(!Policy::FirstN(10).admit_atomic(counter));
                }
            });
        }
    });
    assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
}
