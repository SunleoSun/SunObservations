use serde::Serialize;
use std::sync::Arc;
use sun_observations::{global, observe, JsonlConfig, JsonlSink, Observation, Selection};

#[derive(Serialize, Observation)]
#[observation(namespace = "service::request", tags("production"))]
struct Request {
    path: &'static str,
    status: u16,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = global();
    let config = JsonlConfig::new()
        .flush_ms(1000)
        .file_size_mb(10)
        .trim_fraction(0.10);
    let file = Arc::new(JsonlSink::with_config("observations.jsonl", config)?);
    engine.add_channel_sink("operations", file.clone());
    engine.register_namespace_channel("service", "operations")?;
    engine.set_selection(Selection::new().activate_namespace("service"));

    let emission = observe!(Request {
        path: "/health",
        status: 200,
    });
    let persistence = engine.flush_channels();
    emission?;
    persistence?;
    assert!(file.last_error().is_none());
    assert_eq!(file.stats().records_written, 1);
    Ok(())
}
