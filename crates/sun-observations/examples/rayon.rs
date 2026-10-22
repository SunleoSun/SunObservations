use rayon::prelude::*;
use serde::Serialize;
use std::sync::Arc;
use sun_observations::{
    context::rayon::ContextThreadPool, global, observe, start_scope, Context, MemorySink,
    Observation, Policy, ScopeConfig,
};

#[derive(Serialize, Observation)]
#[observation(namespace = "jobs::item")]
struct Item {
    index: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let memory = Arc::new(MemorySink::new());
    global().add_channel_sink("capture", memory.clone());
    global().register_namespace_channel("jobs", "capture")?;
    let scope = start_scope(
        ScopeConfig::new("pool-owner")
            .activate_namespace("jobs")
            .unwrap()
            .override_policy::<Item>(Policy::first_n(7).unwrap()),
    );
    let pool = ContextThreadPool::with_threads(std::num::NonZeroUsize::new(4).unwrap())?;
    drop(scope);

    let _caller = start_scope(ScopeConfig::new("another-caller"));
    pool.install(|| {
        (0..10).into_par_iter().for_each(|outer| {
            (0..10).into_par_iter().for_each(|inner| {
                assert_eq!(Context::capture().scope_name(), Some("pool-owner"));
                observe!(Item {
                    index: outer * 10 + inner
                })
                .unwrap();
            });
        });
    });
    assert_eq!(Context::capture().scope_name(), Some("another-caller"));
    assert_eq!(memory.len(), 7);
    assert!(memory
        .shared_records()
        .iter()
        .all(|record| record.scope.as_deref() == Some("pool-owner")));
    global().flush_channels()?;
    Ok(())
}
