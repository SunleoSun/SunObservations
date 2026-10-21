//! Observation snapshot, envelope selection, decoding and provenance adapters.
mod dataset;
pub use dataset::*;
#[cfg(test)]
#[path = "grouping_certification.rs"]
mod grouping_certification;
#[cfg(test)]
mod test_support;
