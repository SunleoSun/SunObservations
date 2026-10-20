use crate::*;
use serde::Serialize;
use std::{collections::HashMap, io, sync::Arc};
#[derive(Serialize)]
struct Work {
    id: usize,
}
impl Observation for Work {
    const NAMESPACE: &'static str = "spans::work";
    const LEVEL: ObservationLevel = ObservationLevel::WARN;
    fn message(&self) -> Option<&str> {
        Some("typed start")
    }
}
#[test]
fn concurrent_pairs_freeze_scope_routing_and_budget_through_mutation_and_unwind() {
    let engine = ObservationEngine::new();
    let old = Arc::new(MemorySink::new());
    let new = Arc::new(MemorySink::new());
    engine.register_namespace_channel("spans", "route").unwrap();
    engine.add_channel_sink("route", old.clone());
    let scope = start_scope(
        ScopeConfig::new("origin")
            .activate_all()
            .override_policy::<Work>(Policy::FirstN(101)),
    );
    let context = Context::capture();
    std::thread::scope(|threads| {
        for id in 0..100 {
            let context = context.clone();
            let engine = &engine;
            threads.spawn(move || {
                context.run(|| {
                    let span = engine.span_if_active(|| Work { id }).unwrap();
                    assert!(span.is_active());
                    span.finish().unwrap();
                })
            });
        }
    });
    let last = engine.span_if_active(|| Work { id: 100 }).unwrap();
    assert!(!engine
        .span_if_active::<Work, _>(|| panic!("count once"))
        .unwrap()
        .is_active());
    engine.clear_channel_sinks("route");
    engine.add_channel_sink("route", new.clone());
    engine.set_selection(Selection::new().exclude_namespace("spans"));
    drop(scope);
    last.finish().unwrap();
    assert!(new.is_empty());
    let records = old.records();
    assert_eq!(records.len(), 202);
    let mut pairs = HashMap::<_, Vec<_>>::new();
    for r in &records {
        pairs.entry(r.span.clone().unwrap()).or_default().push(r);
    }
    assert_eq!(pairs.len(), 101);
    for pair in pairs.values() {
        assert_eq!(pair.len(), 2);
        let (start, end) = (pair[0], pair[1]);
        assert_eq!(start.phase, Some(Phase::Start));
        assert_eq!(end.phase, Some(Phase::End));
        assert_eq!(
            (&start.proc, &start.scope, &start.namespace, start.level),
            (&end.proc, &end.scope, &end.namespace, end.level)
        );
        assert_eq!(start.scope.as_deref(), Some("origin"));
        assert!(start.data["id"].is_u64());
        assert_eq!(start.message.as_deref(), Some("typed start"));
        assert!(end.data.is_null());
        assert!(end.message.is_none());
    }
    engine.set_selection(Selection::new().activate_all());
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _span = engine.span_if_active(|| Work { id: 200 }).unwrap();
        panic!("unwind");
    }))
    .is_err());
    assert_eq!(new.len(), 2);
}
struct EndFailure;
impl Sink for EndFailure {
    fn write(&self, record: &ObservationRecord) -> io::Result<()> {
        if record.phase == Some(Phase::End) {
            Err(io::Error::other("end"))
        } else {
            Ok(())
        }
    }
}
#[test]
fn explicit_end_failure_consumes_one_end_and_one_checked_error() {
    let engine = ObservationEngine::new();
    let memory = Arc::new(MemorySink::new());
    engine.set_selection(Selection::new().activate_all());
    engine.register_namespace_channel("spans", "route").unwrap();
    engine.add_channel_sink("route", Arc::new(EndFailure));
    engine.add_channel_sink("route", memory.clone());
    assert!(engine
        .span_if_active(|| Work { id: 0 })
        .unwrap()
        .finish()
        .is_err());
    assert_eq!(engine.sink_error_count(), 1);
    assert_eq!(memory.len(), 2);
}
