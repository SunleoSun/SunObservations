#![doc = include_str!("../README.md")]
extern crate self as sun_observations;

pub mod capture;
pub mod certifications;
pub mod channel;
pub mod context;
pub mod engine;
pub mod evidence;
pub mod level;
mod macros;
pub mod namespace;
pub mod observation;
mod plan;
pub mod policy;
pub mod record;
pub mod routing;
pub mod scope;
pub mod selection;
pub mod sink;
pub mod sinks;
pub mod span;

#[cfg(test)]
mod test_support;

pub use channel::Channel;
pub use context::{Context, ContextGuard};
pub use engine::{global, observe_if_active, ObservationEngine, ObservationError};
pub use level::ObservationLevel;
pub use observation::Observation;
pub use policy::Policy;
pub use record::{ObservationRecord, Phase};
pub use scope::{start_scope, ScopeConfig};
pub use selection::Selection;
pub use sink::{ObservationDelivery, Sink};
pub use sinks::{
    ConsoleSink, JsonlConfig, JsonlSink, JsonlStats, MemorySink, MemorySinkConfig, MemoryStats,
};
pub use span::{span_if_active, SpanGuard};
pub use sun_observations_derive::Observation;

#[doc(hidden)]
pub mod __private {
    pub use serde;
}

#[cfg(test)]
#[path = "macros_certification.rs"]
mod macros_certification;
