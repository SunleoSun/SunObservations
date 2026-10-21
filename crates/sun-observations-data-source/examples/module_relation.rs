use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sun_certifications::{RelationExpectation, RelationMeasure};
use sun_observations::{MemorySink, Observation, ObservationEngine, Selection};
use sun_observations_data_source::ObservationDataset;

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
    let a = inputs.source("producer boundary");
    let b = outputs.source("consumer result");
    let pair = a.compare_with(&b).on(|v| v.row, |v| v.row);
    let values = pair.numeric(|v| v.value, |v| v.value);
    let expected = RelationExpectation::positive(0.99)?
        .bound(RelationMeasure::Slope, Some(1.99), Some(2.01))?
        .bound(RelationMeasure::Intercept, Some(2.99), Some(3.01))?
        .bound(RelationMeasure::ResidualMaxAbsolute, None, Some(1e-10))?;
    sun_certifications::certify("module semantic transfer")
        .check(
            "coverage",
            pair.coverage().compare(sun_certifications::AllValid),
        )
        .check("affine output", values.relation().compare(expected))
        .assert_certify();
    Ok(())
}
