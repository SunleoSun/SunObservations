use crate::*;
use serde::Serialize;
use std::sync::Arc;

#[test]
fn retired_sink_can_reenter_context_and_configuration_during_generation_eviction() {
    let mut certification = sun_observations::certifications::certification(
        "retired sink can reenter context and configuration during generation eviction",
    );
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Weak,
    };
    struct Reentrant {
        engine: Weak<ObservationEngine>,
        dropped: Arc<AtomicBool>,
    }
    impl Sink for Reentrant {
        fn flush(&self) -> std::io::Result<()> {
            Ok(())
        }
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
    let engine = Arc::new(ObservationEngine::isolated());
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
    certification.check_fact_expect(
        "dropped.load(Ordering::Acquire)",
        dropped.load(Ordering::Acquire),
        true,
    );
    certification.assert_certify();
}

#[derive(Serialize, Observation)]
#[observation(namespace = "cache::same")]
struct Event<T: 'static>(T);
#[test]
fn tls_eviction_reuses_shared_plan_and_distinct_types_scopes_engines_do_not_alias() {
    let mut certification = sun_observations::certifications::certification(
        "tls eviction reuses shared plan and distinct types scopes engines do not alias",
    );
    let a = ObservationEngine::isolated();
    let b = ObservationEngine::isolated();
    a.set_selection(Selection::new().activate_all());
    b.set_selection(Selection::new().activate_all());
    let _scope = start_scope(
        ScopeConfig::new("outer")
            .override_policy::<Event<u8>>(Policy::first_n(1).unwrap())
            .override_policy::<Event<u16>>(Policy::first_n(1).unwrap()),
    );
    let original = (*context::cached_plan::<Event<u8>>(&a)).clone();
    certification.check_fact_expect("original.admit()", original.admit(), true);
    let other = context::cached_plan::<Event<u16>>(&a);
    certification.check_fact_expect("other.admit()", other.admit(), true);
    let _ = context::cached_plan::<Event<u32>>(&a);
    let _ = context::cached_plan::<Event<u64>>(&a);
    let _ = context::cached_plan::<Event<i8>>(&a);
    let again = context::cached_plan::<Event<u8>>(&a);
    certification.check_fact_expect(
        "Arc::ptr_eq(&original, &again)",
        Arc::ptr_eq(&original, &again),
        true,
    );
    certification.check_fact_expect("again.admit()", again.admit(), false);
    let independent = context::cached_plan::<Event<u8>>(&b);
    certification.check_fact_expect(
        "Arc::ptr_eq(&original, &independent)",
        Arc::ptr_eq(&original, &independent),
        false,
    );
    certification.check_fact_expect("independent.active", independent.active, true);
    certification.check_fact_expect("independent.admit()", independent.admit(), false);
    {
        let _inner = start_scope(
            ScopeConfig::new("outer").override_policy::<Event<u8>>(Policy::first_n(1).unwrap()),
        );
        certification.check_fact_expect(
            "a.observe_if_active(|| Event(1u8)).unwrap()",
            a.observe_if_active(|| Event(1u8)).unwrap(),
            true,
        );
        certification.check_fact_expect(
            "a.observe_if_active(|| Event(2u8)).unwrap()",
            a.observe_if_active(|| Event(2u8)).unwrap(),
            false,
        );
    }
    certification.check_fact_expect(
        "a.observe_if_active(|| Event(3u8)).unwrap()",
        a.observe_if_active(|| Event(3u8)).unwrap(),
        false,
    );
    certification.assert_certify();
}

mod admission {
    use crate::{
        start_scope, Context, MemorySink, Observation, ObservationEngine, Policy, ScopeConfig,
        Selection,
    };
    use serde::Serialize;
    use std::sync::Arc;
    #[derive(Serialize)]
    struct Event {
        value: usize,
    }
    impl Observation for Event {
        const NAMESPACE: &'static str = "plans::event";
        const TAGS: &'static [&'static str] = &["detail"];
        fn render_text(&self) -> Option<String> {
            panic!("structured plans never render")
        }
    }
    #[test]
    fn mutations_invalidate_selection_routes_and_sink_snapshots_without_resetting_counters() {
        let mut certification = sun_observations::certifications::certification(
            "mutations invalidate selection routes and sink snapshots without resetting counters",
        );
        let engine = ObservationEngine::isolated();
        let memory = Arc::new(MemorySink::new());
        let _scope = start_scope(
            ScopeConfig::new("mutation").override_policy::<Event>(Policy::every_n(2).unwrap()),
        );
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event { value: 0 }).unwrap()",
            engine.observe_if_active(|| Event { value: 0 }).unwrap(),
            false,
        );
        engine.set_selection(Selection::new().activate_namespace("plans").unwrap());
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
        engine
            .register_namespace_channel("plans", "capture")
            .unwrap();
        engine.add_channel_sink("capture", memory.clone());
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event { value: 3 }).unwrap()",
            engine.observe_if_active(|| Event { value: 3 }).unwrap(),
            false,
        );
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event { value: 4 }).unwrap()",
            engine.observe_if_active(|| Event { value: 4 }).unwrap(),
            true,
        );
        let second = Arc::new(MemorySink::new());
        engine.add_channel_sink("capture", second.clone());
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event { value: 5 }).unwrap()",
            engine.observe_if_active(|| Event { value: 5 }).unwrap(),
            false,
        );
        certification.check_fact_expect(
            "engine.observe_if_active(|| Event { value: 6 }).unwrap()",
            engine.observe_if_active(|| Event { value: 6 }).unwrap(),
            true,
        );
        certification.check_fact_expect("memory.len()", &(memory.len()), &(2));
        certification.check_fact_expect("second.len()", &(second.len()), &(1));
        certification.check_fact_expect(
            "Arc::ptr_eq( &memory.shared_records()[1], &second.shared_records()[0] )",
            Arc::ptr_eq(&memory.shared_records()[1], &second.shared_records()[0]),
            true,
        );
        engine.set_selection(
            Selection::new()
                .activate_all()
                .exclude_namespace("plans")
                .unwrap(),
        );
        certification.check_fact_expect(
            "engine .observe_if_active::<Event, _>(|| panic!(\"new exclusion\")) .unwrap()",
            engine
                .observe_if_active::<Event, _>(|| panic!("new exclusion"))
                .unwrap(),
            false,
        );
        certification.assert_certify();
    }

    #[test]
    fn route_addition_attachment_and_detachment_each_invalidate_independently() {
        let mut certification = sun_observations::certifications::certification(
            "route addition attachment and detachment each invalidate independently",
        );
        let engine = ObservationEngine::isolated();
        engine.set_selection(Selection::new().activate_all());
        let a = Arc::new(MemorySink::new());
        let b = Arc::new(MemorySink::new());
        engine.add_channel_sink("capture", a.clone());
        engine.observe_if_active(|| Event { value: 1 }).unwrap();
        certification.check_fact_expect("a.is_empty()", a.is_empty(), true);
        engine
            .register_namespace_channel("plans", "capture")
            .unwrap();
        engine.observe_if_active(|| Event { value: 2 }).unwrap();
        certification.check_fact_expect("a.len()", &(a.len()), &(1));
        engine.add_channel_sink("capture", b.clone());
        engine.observe_if_active(|| Event { value: 3 }).unwrap();
        certification.check_fact_expect("b.len()", &(b.len()), &(1));
        engine.clear_channel_sinks("capture");
        engine.observe_if_active(|| Event { value: 4 }).unwrap();
        certification.check_fact_expect("a.len()", &(a.len()), &(2));
        certification.check_fact_expect("b.len()", &(b.len()), &(1));
        certification.assert_certify();
    }

    #[test]
    fn shared_cache_generation_changes_are_visible_to_already_warm_children() {
        let mut certification = sun_observations::certifications::certification(
            "shared cache generation changes are visible to already warm children",
        );
        let engine = Arc::new(ObservationEngine::isolated());
        let memory = Arc::new(MemorySink::new());
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let _scope = start_scope(ScopeConfig::new("generation"));
        let context = Context::capture();
        let child_engine = engine.clone();
        let child_barrier = barrier.clone();
        let child = std::thread::spawn(move || {
            context.run(|| {
                let before = child_engine
                    .observe_if_active::<Event, _>(|| panic!("inactive"))
                    .unwrap();
                child_barrier.wait();
                child_barrier.wait();
                let after = child_engine
                    .observe_if_active(|| Event { value: 42 })
                    .unwrap();
                (before, after)
            })
        });
        barrier.wait();
        engine.set_selection(Selection::new().activate_tags(["detail"]).unwrap());
        engine.add_channel_sink("new", memory.clone());
        engine.register_namespace_channel("plans", "new").unwrap();
        barrier.wait();
        certification.check_fact_expect(
            "warm child observes generation update",
            child.join().unwrap(),
            (false, true),
        );
        certification.check_fact_expect(
            "memory.records()[0].scope.as_deref()",
            &(memory.records()[0].scope.as_deref()),
            &(Some("generation")),
        );
        certification.assert_certify();
    }

    #[test]
    fn same_sink_on_two_matching_channels_receives_shared_duplicate_deliveries() {
        let mut certification = sun_observations::certifications::certification(
            "same sink on two matching channels receives shared duplicate deliveries",
        );
        let engine = ObservationEngine::isolated();
        let memory = Arc::new(MemorySink::new());
        engine.add_channel_sink("a", memory.clone());
        engine.add_channel_sink("b", memory.clone());
        engine.register_namespace_channel("plans", "a").unwrap();
        engine
            .register_namespace_channel("plans::event", "b")
            .unwrap();
        engine.set_selection(Selection::new().activate_all());
        engine.observe_if_active(|| Event { value: 1 }).unwrap();
        let records = memory.shared_records();
        certification.check_fact_expect("records.len()", &(records.len()), &(2));
        certification.check_fact_expect(
            "Arc::ptr_eq(&records[0], &records[1])",
            Arc::ptr_eq(&records[0], &records[1]),
            true,
        );
        certification.assert_certify();
    }

    #[test]
    fn engine_identity_and_nested_scope_cache_do_not_alias() {
        let mut certification = sun_observations::certifications::certification(
            "engine identity and nested scope cache do not alias",
        );
        let a = ObservationEngine::isolated();
        let b = ObservationEngine::isolated();
        a.set_selection(Selection::new().activate_all());
        let _outer = start_scope(
            ScopeConfig::new("outer").override_policy::<Event>(Policy::first_n(1).unwrap()),
        );
        certification.check_fact_expect(
            "a.observe_if_active(|| Event { value: 1 }).unwrap()",
            a.observe_if_active(|| Event { value: 1 }).unwrap(),
            true,
        );
        certification.check_fact_expect(
            "b .observe_if_active::<Event, _>(|| panic!(\"other engine\")) .unwrap()",
            b.observe_if_active::<Event, _>(|| panic!("other engine"))
                .unwrap(),
            false,
        );
        {
            let _inner = start_scope(ScopeConfig::new("inner"));
            certification.check_fact_expect(
                "a.observe_if_active(|| Event { value: 2 }).unwrap()",
                a.observe_if_active(|| Event { value: 2 }).unwrap(),
                true,
            );
        }
        certification.check_fact_expect(
            "a .observe_if_active::<Event, _>(|| panic!(\"outer counter preserved\")) .unwrap()",
            a.observe_if_active::<Event, _>(|| panic!("outer counter preserved"))
                .unwrap(),
            false,
        );
        certification.assert_certify();
    }
}
