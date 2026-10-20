use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sun_observations::{MemorySink, Observation, ObservationEngine, Selection};
use sun_observations_analysis::{ObservationDataset, RelationExpectation, RelationMeasure};

#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "module::input")]
struct ModuleInput {
    row: u64,
    value: f64,
}
#[derive(Serialize, Deserialize, Observation)]
#[observation(namespace = "module::output")]
struct ModuleOutput {
    row: u64,
    value: f64,
}

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = ObservationEngine::new();
    let memory = Arc::new(MemorySink::new());
    engine.add_channel_sink("evidence", memory.clone());
    engine.register_namespace_channel("module", "evidence")?;
    engine.set_selection(Selection::new().activate_namespace("module"));
    for row in 0..8 {
        engine.observe_if_active(|| ModuleInput {
            row,
            value: row as f64,
        })?;
    }
    for row in (0..8).rev() {
        engine.observe_if_active(|| ModuleOutput {
            row,
            value: 2. * row as f64 + 3.,
        })?;
    }
    engine.flush_channels()?;
    let dataset = ObservationDataset::from_memory(&memory);
    let inputs = dataset.typed::<ModuleInput>()?;
    let outputs = dataset.typed::<ModuleOutput>()?;
    let joined = inputs.join_by(&outputs, |v| v.row, |v| v.row);
    let report = joined.numeric_relation(|v| v.value, |v| v.value, 3)?;
    let expected = RelationExpectation::positive(0.99)?
        .bound(RelationMeasure::Slope, Some(1.99), Some(2.01))?
        .bound(RelationMeasure::Intercept, Some(2.99), Some(3.01))?
        .bound(RelationMeasure::ResidualMaxAbsolute, None, Some(1e-10))?;
    expected.evaluate(report.clone())?;
    let wrong_direction = RelationExpectation::negative(-0.9)?
        .evaluate(report)
        .unwrap_err();
    println!("{}", serde_json::to_string_pretty(&wrong_direction)?);
    Ok(())
}
