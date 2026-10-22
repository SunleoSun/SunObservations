use serde_json::Value;
use std::sync::Arc;
use sun_observations::{ObservationLevel, ObservationRecord};

pub fn record(namespace: &str, data: Value) -> Arc<ObservationRecord> {
    Arc::new(ObservationRecord {
        ts: "2026-10-20T00:00:00.000Z".into(),
        proc: "test".into(),
        proc_ms: 0,
        scope: None,
        namespace: namespace.into(),
        level: ObservationLevel::INFO,
        data,
        message: None,
        phase: None,
        span: None,
    })
}

pub fn shuffle<T>(values: &mut [T], mut state: u64) {
    for i in (1..values.len()).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        values.swap(i, (state as usize) % (i + 1));
    }
}
