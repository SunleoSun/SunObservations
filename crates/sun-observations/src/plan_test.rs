use crate::{
    context, start_scope, Context, MemorySink, Observation, ObservationEngine, Policy, ScopeConfig,
    Selection,
};
use serde::Serialize;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::Arc,
};

struct AllocationCounter;
thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }
#[global_allocator]
static ALLOCATOR: AllocationCounter = AllocationCounter;
unsafe impl GlobalAlloc for AllocationCounter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| {
            if let Some(n) = count.get() {
                count.set(Some(n + 1));
            }
        });
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| {
            if let Some(n) = count.get() {
                count.set(Some(n + 1));
            }
        });
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}
#[derive(Serialize, Observation)]
#[observation(namespace = "plans::event", tags("detail"))]
struct Event {
    value: usize,
}

#[test]
fn warm_rejections_allocate_nothing_and_do_not_build_payloads() {
    let engine = ObservationEngine::new();
    for config in [
        ScopeConfig::new("inactive"),
        ScopeConfig::new("excluded")
            .activate_all()
            .exclude_tags(["detail"]),
        ScopeConfig::new("never")
            .activate_all()
            .override_policy::<Event>(Policy::Never),
        ScopeConfig::new("every")
            .activate_all()
            .override_policy::<Event>(Policy::EveryN(u64::MAX)),
    ] {
        let _scope = start_scope(config);
        assert!(!engine
            .observe_if_active::<Event, _>(|| panic!("lazy"))
            .unwrap());
        ALLOCATIONS.with(|count| count.set(Some(0)));
        for _ in 0..10000 {
            assert!(!engine
                .observe_if_active::<Event, _>(|| panic!("lazy"))
                .unwrap());
        }
        let count = ALLOCATIONS.with(|count| count.replace(None).unwrap());
        assert_eq!(count, 0);
    }
}

#[test]
fn parent_resolution_is_shared_with_children_and_tiny_cache_reuses_it() {
    let engine = Arc::new(ObservationEngine::new());
    let _scope = start_scope(ScopeConfig::new("shared").activate_all());
    let parent = (*context::cached_plan::<Event>(&engine)).clone();
    let context = Context::capture();
    std::thread::scope(|threads| {
        for _ in 0..8 {
            let context = context.clone();
            let parent = parent.clone();
            let engine = engine.clone();
            threads.spawn(move || {
                context.run(|| {
                    let first = context::cached_plan::<Event>(&engine);
                    let second = context::cached_plan::<Event>(&engine);
                    assert!(Arc::ptr_eq(&parent, &first));
                    assert!(std::rc::Rc::ptr_eq(&first, &second));
                })
            });
        }
    });
    assert_eq!(
        context::current_scope().unwrap().plans.lock().plans.len(),
        1
    );
}

#[test]
fn mutations_invalidate_selection_routes_and_sink_snapshots_without_resetting_counters() {
    let engine = ObservationEngine::new();
    let memory = Arc::new(MemorySink::new());
    let _scope =
        start_scope(ScopeConfig::new("mutation").override_policy::<Event>(Policy::EveryN(2)));
    assert!(!engine.observe_if_active(|| Event { value: 0 }).unwrap());
    engine.set_selection(Selection::new().activate_namespace("plans"));
    assert!(!engine.observe_if_active(|| Event { value: 1 }).unwrap());
    assert!(engine.observe_if_active(|| Event { value: 2 }).unwrap());
    engine
        .register_namespace_channel("plans", "capture")
        .unwrap();
    engine.add_channel_sink("capture", memory.clone());
    assert!(!engine.observe_if_active(|| Event { value: 3 }).unwrap());
    assert!(engine.observe_if_active(|| Event { value: 4 }).unwrap());
    let second = Arc::new(MemorySink::new());
    engine.add_channel_sink("capture", second.clone());
    assert!(!engine.observe_if_active(|| Event { value: 5 }).unwrap());
    assert!(engine.observe_if_active(|| Event { value: 6 }).unwrap());
    assert_eq!(memory.len(), 2);
    assert_eq!(second.len(), 1);
    assert!(Arc::ptr_eq(
        &memory.shared_records()[1],
        &second.shared_records()[0]
    ));
    engine.set_selection(Selection::new().activate_all().exclude_namespace("plans"));
    assert!(!engine
        .observe_if_active::<Event, _>(|| panic!("new exclusion"))
        .unwrap());
}

#[test]
fn route_addition_attachment_and_detachment_each_invalidate_independently() {
    let engine = ObservationEngine::new();
    engine.set_selection(Selection::new().activate_all());
    let a = Arc::new(MemorySink::new());
    let b = Arc::new(MemorySink::new());
    engine.add_channel_sink("capture", a.clone());
    engine.observe_if_active(|| Event { value: 1 }).unwrap();
    assert!(a.is_empty());
    engine
        .register_namespace_channel("plans", "capture")
        .unwrap();
    engine.observe_if_active(|| Event { value: 2 }).unwrap();
    assert_eq!(a.len(), 1);
    engine.add_channel_sink("capture", b.clone());
    engine.observe_if_active(|| Event { value: 3 }).unwrap();
    assert_eq!(b.len(), 1);
    engine.clear_channel_sinks("capture");
    engine.observe_if_active(|| Event { value: 4 }).unwrap();
    assert_eq!(a.len(), 2);
    assert_eq!(b.len(), 1);
}

#[test]
fn shared_cache_generation_changes_are_visible_to_already_warm_children() {
    let engine = Arc::new(ObservationEngine::new());
    let memory = Arc::new(MemorySink::new());
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let _scope = start_scope(ScopeConfig::new("generation"));
    let context = Context::capture();
    let child_engine = engine.clone();
    let child_barrier = barrier.clone();
    let child = std::thread::spawn(move || {
        context.run(|| {
            assert!(!child_engine
                .observe_if_active::<Event, _>(|| panic!("inactive"))
                .unwrap());
            child_barrier.wait();
            child_barrier.wait();
            assert!(child_engine
                .observe_if_active(|| Event { value: 42 })
                .unwrap());
        })
    });
    barrier.wait();
    engine.set_selection(Selection::new().activate_tags(["detail"]));
    engine.add_channel_sink("new", memory.clone());
    engine.register_namespace_channel("plans", "new").unwrap();
    barrier.wait();
    child.join().unwrap();
    assert_eq!(memory.records()[0].scope.as_deref(), Some("generation"));
}

#[test]
fn same_sink_on_two_matching_channels_receives_shared_duplicate_deliveries() {
    let engine = ObservationEngine::new();
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
    assert_eq!(records.len(), 2);
    assert!(Arc::ptr_eq(&records[0], &records[1]));
}

#[test]
fn engine_identity_and_nested_scope_cache_do_not_alias() {
    let a = ObservationEngine::new();
    let b = ObservationEngine::new();
    a.set_selection(Selection::new().activate_all());
    let _outer = start_scope(ScopeConfig::new("outer").override_policy::<Event>(Policy::FirstN(1)));
    assert!(a.observe_if_active(|| Event { value: 1 }).unwrap());
    assert!(!b
        .observe_if_active::<Event, _>(|| panic!("other engine"))
        .unwrap());
    {
        let _inner = start_scope(ScopeConfig::new("inner"));
        assert!(a.observe_if_active(|| Event { value: 2 }).unwrap());
    }
    assert!(!a
        .observe_if_active::<Event, _>(|| panic!("outer counter preserved"))
        .unwrap());
}
