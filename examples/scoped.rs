use serde::Serialize;
use std::sync::Arc;
use sun_observations::{
    context, global, observe, observe_span, start_scope, MemorySink, Observation, Policy,
    ScopeConfig,
};

#[derive(Serialize, Observation)]
#[observation(namespace = "compute::batch", tags("detail"))]
struct Batch {
    index: usize,
}

#[derive(Serialize, Observation)]
#[observation(namespace = "compute::run")]
struct Run {
    items: usize,
}

fn work() {
    observe!(Batch { index: 1 }).unwrap();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let memory = Arc::new(MemorySink::new());
    global().channel("capture").add_sink(memory.clone());
    global().register_namespace_channel("compute", "capture")?;
    let _scope = start_scope(
        ScopeConfig::new("run-a")
            .activate_namespace("compute")
            .override_policy::<Batch>(Policy::FirstN(1)),
    );
    {
        let _span = observe_span!(Run { items: 2 })?;
        context::thread::spawn(work).join().unwrap();
        work();
    }
    assert_eq!(memory.len(), 3);
    Ok(())
}
