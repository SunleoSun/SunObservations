use serde::Serialize;
use std::sync::Arc;
use sun_observations::{
    global, observe_span, start_scope, MemorySink, Observation, ObservationLevel, Phase, Policy,
    ScopeConfig,
};

#[derive(Serialize)]
struct Fit {
    samples: usize,
}

impl Observation for Fit {
    const NAMESPACE: &'static str = "encoder::fit";
    const LEVEL: ObservationLevel = ObservationLevel::INFO;

    fn message(&self) -> Option<&str> {
        Some("training fit")
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let memory = Arc::new(MemorySink::new());
    global().add_channel_sink("capture", memory.clone());
    global().register_namespace_channel("encoder", "capture")?;
    let scope = start_scope(
        ScopeConfig::new("fit-42")
            .activate_namespace("encoder")
            .unwrap()
            .override_policy::<Fit>(Policy::first_n(1).unwrap()),
    );
    let span = observe_span!(Fit { samples: 128 })?;
    assert!(span.is_active());
    assert!(!observe_span!(Fit { samples: 256 })?.is_active());
    drop(scope);
    span.finish()?;
    global().flush_channels()?;

    let records = memory.shared_records();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].phase, Some(Phase::Start));
    assert_eq!(records[1].phase, Some(Phase::End));
    assert_eq!(records[0].span, records[1].span);
    assert_eq!(records[0].scope, records[1].scope);
    assert_eq!(records[0].message.as_deref(), Some("training fit"));
    assert!(records[1].data.is_null());
    assert!(records[1].message.is_none());
    Ok(())
}
