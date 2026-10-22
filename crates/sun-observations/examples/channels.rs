use serde::Serialize;
use std::sync::Arc;
use sun_observations::{
    global, observe, start_scope, JsonlSink, MemorySink, Observation, ScopeConfig,
};

#[derive(Serialize, Observation)]
#[observation(namespace = "service::jobs::completed")]
struct Completed {
    count: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = global();
    let memory = Arc::new(MemorySink::new());
    engine.add_channel_sink("all", Arc::new(JsonlSink::from_writer(std::io::stdout())));
    engine.add_channel_sink("jobs", memory.clone());
    engine.add_channel_sink("jobs", Arc::new(JsonlSink::from_writer(std::io::sink())));
    engine.register_namespace_channel("service", "all")?;
    engine.register_namespace_channel("service::jobs", "jobs")?;
    let _scope = start_scope(
        ScopeConfig::new("jobs-a")
            .activate_namespace("service")
            .unwrap(),
    );
    observe!(Completed { count: 10 })?;
    assert_eq!(memory.len(), 1);
    engine.flush_channels()?;
    Ok(())
}
