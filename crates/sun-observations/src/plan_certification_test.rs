use crate::*;
use serde::Serialize;
use std::sync::Arc;

#[test]
fn retired_sink_can_reenter_context_and_configuration_during_generation_eviction() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Weak,
    };
    struct Reentrant {
        engine: Weak<ObservationEngine>,
        dropped: Arc<AtomicBool>,
    }
    impl Sink for Reentrant {
        fn write(&self, _: &ObservationRecord) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Drop for Reentrant {
        fn drop(&mut self) {
            let engine = self.engine.upgrade().unwrap();
            engine.set_selection(Selection::new().activate_all());
            Context::default().run(|| {
                assert!(engine.observe_if_active(|| Event(0i64)).unwrap());
            });
            self.dropped.store(true, Ordering::Release);
        }
    }
    let engine = Arc::new(ObservationEngine::new());
    engine.set_selection(Selection::new().activate_all());
    let dropped = Arc::new(AtomicBool::new(false));
    engine.add_channel_sink(
        "old",
        Arc::new(Reentrant {
            engine: Arc::downgrade(&engine),
            dropped: dropped.clone(),
        }),
    );
    engine.register_namespace_channel("cache", "old").unwrap();
    let _scope = start_scope(ScopeConfig::new("reentrant"));
    engine.observe_if_active(|| Event(0u8)).unwrap();
    engine.clear_channel_sinks("old");
    engine.observe_if_active(|| Event(1u8)).unwrap();
    engine.observe_if_active(|| Event(1u16)).unwrap();
    engine.observe_if_active(|| Event(1u32)).unwrap();
    engine.observe_if_active(|| Event(1u64)).unwrap();
    assert!(dropped.load(Ordering::Acquire));
}

#[derive(Serialize, Observation)]
#[observation(namespace = "cache::same")]
struct Event<T: 'static>(T);
#[test]
fn tls_eviction_reuses_shared_plan_and_distinct_types_scopes_engines_do_not_alias() {
    let a = ObservationEngine::new();
    let b = ObservationEngine::new();
    a.set_selection(Selection::new().activate_all());
    b.set_selection(Selection::new().activate_all());
    let _scope = start_scope(
        ScopeConfig::new("outer")
            .override_policy::<Event<u8>>(Policy::FirstN(1))
            .override_policy::<Event<u16>>(Policy::FirstN(1)),
    );
    let original = (*context::cached_plan::<Event<u8>>(&a)).clone();
    assert!(original.admit());
    let other = context::cached_plan::<Event<u16>>(&a);
    assert!(other.admit());
    let _ = context::cached_plan::<Event<u32>>(&a);
    let _ = context::cached_plan::<Event<u64>>(&a);
    let _ = context::cached_plan::<Event<i8>>(&a);
    let again = context::cached_plan::<Event<u8>>(&a);
    assert!(Arc::ptr_eq(&original, &again));
    assert!(!again.admit());
    let independent = context::cached_plan::<Event<u8>>(&b);
    assert!(!Arc::ptr_eq(&original, &independent));
    assert!(independent.active);
    assert!(!independent.admit());
    {
        let _inner =
            start_scope(ScopeConfig::new("outer").override_policy::<Event<u8>>(Policy::FirstN(1)));
        assert!(a.observe_if_active(|| Event(1u8)).unwrap());
        assert!(!a.observe_if_active(|| Event(2u8)).unwrap());
    }
    assert!(!a.observe_if_active(|| Event(3u8)).unwrap());
}
