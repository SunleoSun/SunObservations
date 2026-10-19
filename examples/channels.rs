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
    engine
        .channel("all")
        .add_sink(Arc::new(JsonlSink::new(std::io::stdout())));
    engine.channel("jobs").add_sink(memory.clone());
    engine
        .channel("jobs")
        .add_sink(Arc::new(JsonlSink::new(std::io::sink())));
    engine.register_namespace_channel("service", "all")?;
    engine.register_namespace_channel("service::jobs", "jobs")?;
    let _scope = start_scope(ScopeConfig::new("jobs-a").activate_namespace("service"));
    observe!(Completed { count: 10 })?;
    assert_eq!(memory.len(), 1);
    engine.flush()?;
    Ok(())
}
