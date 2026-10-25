//! Generic, typed semantic proofs over ordinary Rust evidence, independent of runtime types and state.
//! Facts never assert; expectations and final proof aggregation own acceptance.
mod anomaly;
mod certification;
mod criteria;
mod domain;
mod execution;
pub use domain::*;
pub use execution::*;
pub use sun_observations_derive::{domain, fact, DomainInput};
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
#[path = "../composition_certifications/acceptance_aggregation_certification.rs"]
mod acceptance_aggregation_certification;
#[cfg(test)]
#[path = "../composition_certifications/client_api_ergonomics_certification.rs"]
mod client_api_ergonomics_certification;
#[cfg(test)]
mod domain_certification;
#[cfg(test)]
#[path = "../composition_certifications/domain_execution_certification.rs"]
mod domain_execution_certification;
#[cfg(test)]
#[path = "../composition_certifications/prepared_evidence_workflow_certification.rs"]
mod prepared_evidence_workflow_certification;
#[cfg(test)]
mod test_support;
