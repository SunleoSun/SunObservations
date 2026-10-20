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
pub fn close(actual: f64, expected: f64) {
    assert!(actual.is_finite() && expected.is_finite());
    assert!(
        (actual - expected).abs() <= 1e-10 * (1.0 + expected.abs()),
        "{actual} != {expected}"
    );
}
pub fn shuffle<T>(values: &mut [T], mut state: u64) {
    for i in (1..values.len()).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        values.swap(i, (state as usize) % (i + 1));
    }
}
pub fn median(values: &[f64]) -> f64 {
    let mut values = values.to_vec();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if values.len() % 2 == 1 {
        values[values.len() / 2]
    } else {
        (values[values.len() / 2 - 1] + values[values.len() / 2]) / 2.0
    }
}
pub fn reference_linear(values: &[(f64, f64)]) -> (f64, f64, f64, f64, f64) {
    let n = values.len() as f64;
    let mx = values.iter().map(|p| p.0).sum::<f64>() / n;
    let my = values.iter().map(|p| p.1).sum::<f64>() / n;
    let xx = values.iter().map(|p| (p.0 - mx).powi(2)).sum::<f64>();
    let yy = values.iter().map(|p| (p.1 - my).powi(2)).sum::<f64>();
    let xy = values.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>();
    (
        xy / (xx * yy).sqrt(),
        xy / n,
        xy / xx,
        my - xy / xx * mx,
        xy * xy / (xx * yy),
    )
}
pub fn reference_rank(values: &[f64]) -> Vec<f64> {
    values
        .iter()
        .map(|x| {
            let less = values.iter().filter(|v| *v < x).count();
            let equal = values.iter().filter(|v| *v == x).count();
            less as f64 + (equal as f64 + 1.0) / 2.0
        })
        .collect()
}
