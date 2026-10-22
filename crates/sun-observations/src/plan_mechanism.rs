use crate::{
    context, start_scope, Context, Observation, ObservationEngine, Policy, ScopeConfig, Selection,
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
fn delivery_and_human_fanout_clones_allocate_nothing() {
    let record = Arc::new(crate::test_support::record("plans::delivery"));
    let text = "formatted text".to_owned();
    let buffer = text.as_ptr();
    let shared = Arc::new(text);
    assert_eq!(shared.as_ptr(), buffer);
    ALLOCATIONS.with(|count| count.set(Some(0)));
    let delivery = crate::ObservationDelivery::new(record.clone(), Some(shared.clone()));
    let copy = delivery.clone();
    let structured = crate::ObservationDelivery::new(record.clone(), None);
    let count = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(count, 0);
    assert!(Arc::ptr_eq(delivery.shared_record(), copy.shared_record()));
    assert!(Arc::ptr_eq(
        delivery.shared_render_text().unwrap(),
        copy.shared_render_text().unwrap()
    ));
    assert_eq!(delivery.render_text().unwrap().as_ptr(), buffer);
    assert!(structured.render_text().is_none());
}

#[test]
fn structured_dispatch_and_stack_human_delivery_have_equal_allocation_cost() {
    struct Discard(bool);
    impl crate::Sink for Discard {
        fn flush(&self) -> std::io::Result<()> {
            Ok(())
        }
        fn write(&self, _: &crate::ObservationRecord) -> std::io::Result<()> {
            Ok(())
        }
        fn wants_render_text(&self) -> bool {
            self.0
        }
    }
    #[derive(Serialize, Observation)]
    #[observation(namespace = "plans::allocation")]
    struct Payload {
        value: u64,
    }
    let engine = ObservationEngine::isolated();
    engine.set_selection(Selection::new().activate_all());
    engine.register_namespace_channel("plans", "all").unwrap();
    let counts: Vec<_> = [false, true]
        .into_iter()
        .map(|human| {
            engine.clear_channel_sinks("all");
            engine.add_channel_sink("all", Arc::new(Discard(human)));
            engine.observe_if_active(|| Payload { value: 1 }).unwrap();
            ALLOCATIONS.with(|count| count.set(Some(0)));
            for _ in 0..100 {
                engine.observe_if_active(|| Payload { value: 1 }).unwrap();
            }
            ALLOCATIONS.with(|count| count.replace(None).unwrap())
        })
        .collect();
    assert!(counts[0] > 0);
    assert_eq!(counts[0], counts[1]);
}

#[test]
fn warm_rejections_allocate_nothing_and_do_not_build_payloads() {
    let engine = ObservationEngine::isolated();
    for config in [
        ScopeConfig::new("inactive"),
        ScopeConfig::new("excluded")
            .activate_all()
            .exclude_tags(["detail"])
            .unwrap(),
        ScopeConfig::new("never")
            .activate_all()
            .override_policy::<Event>(Policy::Never),
        ScopeConfig::new("every")
            .activate_all()
            .override_policy::<Event>(Policy::every_n(u64::MAX).unwrap()),
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
    let engine = Arc::new(ObservationEngine::isolated());
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
