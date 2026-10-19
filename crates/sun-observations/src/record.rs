use crate::ObservationLevel;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Start,
    End,
}

/// The v1 persisted envelope. Tags and channel names are deliberately absent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservationRecord {
    pub ts: String,
    pub proc: String,
    pub proc_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scope: Option<String>,
    pub namespace: String,
    pub level: ObservationLevel,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub phase: Option<Phase>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub span: Option<String>,
}

#[cfg(test)]
#[path = "record_test.rs"]
mod tests;
