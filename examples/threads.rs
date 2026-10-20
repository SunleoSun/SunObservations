use serde::Serialize;
use std::sync::Arc;
use sun_observations::{
    context, global, observe, start_scope, Context, MemorySink, Observation, ObservationError,
    Policy, ScopeConfig,
};

#[derive(Serialize, Observation)]
#[observation(namespace = "jobs::completed")]
struct Completed {
    job: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let memory = Arc::new(MemorySink::new());
    global().add_channel_sink("capture", memory.clone());
    global().register_namespace_channel("jobs", "capture")?;
    let scope = start_scope(
        ScopeConfig::new("job-42")
            .activate_namespace("jobs")
            .override_policy::<Completed>(Policy::FirstN(2)),
    );
    let captured = Context::capture();

    std::thread::spawn(|| {
        assert_eq!(Context::capture().scope_name(), None);
        assert!(!observe!(Completed { job: 0 }).unwrap());
    })
    .join()
    .unwrap();

    context::thread::spawn(|| -> Result<(), ObservationError> {
        assert_eq!(Context::capture().scope_name(), Some("job-42"));
        assert!(observe!(Completed { job: 1 })?);
        context::thread::spawn(|| observe!(Completed { job: 2 }))
            .join()
            .unwrap()?;
        Ok(())
    })
    .join()
    .unwrap()?;

    drop(scope);
    captured.run(|| {
        assert_eq!(Context::capture().scope_name(), Some("job-42"));
        assert!(!observe!(Completed { job: 3 }).unwrap());
    });
    assert_eq!(Context::capture().scope_name(), None);
    assert_eq!(memory.len(), 2);
    assert!(memory
        .shared_records()
        .iter()
        .all(|record| record.scope.as_deref() == Some("job-42")));
    global().flush_channels()?;
    Ok(())
}
