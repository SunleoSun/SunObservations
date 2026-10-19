use crate::{MemorySink, ObservationEngine, ObservationLevel, ObservationRecord};
use std::sync::Arc;

pub fn capture(engine: &ObservationEngine, namespace: &str) -> Arc<MemorySink> {
    let sink = Arc::new(MemorySink::new());
    engine.channel(namespace).add_sink(sink.clone());
    engine
        .register_namespace_channel(namespace, namespace)
        .unwrap();
    sink
}

pub fn record(namespace: &str) -> ObservationRecord {
    ObservationRecord {
        ts: "2026-10-20T00:00:00.000Z".into(),
        proc: "1234_opaque".into(),
        proc_ms: 42,
        scope: None,
        namespace: namespace.into(),
        level: ObservationLevel::INFO,
        data: serde_json::json!({"value": 7}),
        message: None,
        phase: None,
        span: None,
    }
}
