//! Generic, typed semantic proofs over ordinary Rust evidence. Measurements never assert.
mod anomaly;
mod certification;
mod criteria;
mod expectations;
mod numerical;
pub use expectations::*;
pub use numerical::*;
mod grouping;
mod integrity;
mod join;
mod lag;
mod relation;
mod source;
mod stats;
pub use anomaly::*;
pub use certification::*;
pub use criteria::*;
pub use grouping::*;
pub use integrity::*;
pub use join::*;
pub use lag::*;
pub use relation::*;
pub use source::*;
pub use stats::*;
#[cfg(test)]
mod ergonomics_certification;
#[cfg(test)]
#[path = "framework_certification.rs"]
mod framework_certification;
#[cfg(test)]
mod test_support;
