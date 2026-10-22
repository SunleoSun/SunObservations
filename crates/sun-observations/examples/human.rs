use serde::Serialize;
use std::sync::Arc;
use sun_observations::{global, observe, ConsoleSink, Observation, Selection};

#[derive(Serialize, Observation)]
#[observation(namespace = "example::status")]
struct Status {
    state: &'static str,
}

#[derive(Serialize)]
struct TableStart {
    columns: [&'static str; 3],
}
impl Observation for TableStart {
    const NAMESPACE: &'static str = "example::table::start";
    fn render_text(&self) -> Option<String> {
        Some("Epoch | Loss     | Accuracy\n------+----------+---------".into())
    }
}
#[derive(Serialize)]
struct Epoch {
    epoch: u64,
    loss: f64,
    accuracy: f64,
}
impl Observation for Epoch {
    const NAMESPACE: &'static str = "example::table::epoch";
    fn render_text(&self) -> Option<String> {
        Some(format!(
            "{:>5} | {:8.4} | {:8.4}",
            self.epoch, self.loss, self.accuracy
        ))
    }
}
#[derive(Serialize)]
struct TableEnd {
    epochs: u64,
}
impl Observation for TableEnd {
    const NAMESPACE: &'static str = "example::table::end";
    fn render_text(&self) -> Option<String> {
        Some("------+----------+---------".into())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = global();
    engine.set_selection(Selection::new().activate_namespace("example"));
    engine.register_namespace_channel("example::status", "status")?;
    engine.register_namespace_channel("example::table", "table")?;
    engine.add_channel_sink("status", Arc::new(ConsoleSink::stderr()));
    engine.add_channel_sink("table", Arc::new(ConsoleSink::stdout()));
    observe!(Status { state: "training" })?;
    observe!(TableStart {
        columns: ["epoch", "loss", "accuracy"],
    })?;
    for epoch in 1..=2 {
        observe!(Epoch {
            epoch,
            loss: 0.38 / epoch as f64,
            accuracy: 0.61 + epoch as f64 * 0.02,
        })?;
    }
    observe!(TableEnd { epochs: 2 })?;
    engine.flush_channels()?;
    Ok(())
}
