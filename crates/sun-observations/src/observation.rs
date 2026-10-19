use crate::ObservationLevel;
use serde::Serialize;

/// Stable semantic metadata; deriving this trait requires an explicit namespace.
pub trait Observation: Serialize + 'static {
    const NAMESPACE: &'static str;
    const LEVEL: ObservationLevel = ObservationLevel::INFO;
    const TAGS: &'static [&'static str] = &[];

    /// Optional human-readable envelope message, independent of structured data.
    fn message(&self) -> Option<&str> {
        None
    }
}

#[cfg(test)]
#[path = "observation_test.rs"]
mod tests;
