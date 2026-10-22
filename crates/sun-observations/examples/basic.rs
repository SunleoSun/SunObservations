use serde::Serialize;
use std::sync::Arc;
use sun_observations::{global, observe, JsonlSink, Observation, Selection};

#[derive(Serialize, Observation)]
#[observation(namespace = "service::request", level = INFO, tags("detail"))]
struct Request {
    path: &'static str,
    status: u16,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = global();
    engine.add_channel_sink(
        "console",
        Arc::new(JsonlSink::from_writer(std::io::stdout())),
    );
    engine.register_namespace_channel("service", "console")?;
    engine.set_selection(Selection::new().activate_namespace("service").unwrap());
    observe!(Request {
        path: "/health",
        status: 200
    })?;
    engine.flush_channels()?;
    Ok(())
}
