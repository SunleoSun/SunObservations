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
    /// Monotonic nanoseconds since this engine initialized, not wall-clock time.
    pub proc_ns: u64,
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

/// FORMAT v1 envelope or per-record lifecycle violation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordValidationError {
    Timestamp,
    Namespace(crate::namespace::InvalidNamespace),
    Lifecycle,
}
impl std::fmt::Display for RecordValidationError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "invalid Observation record: {self:?}")
    }
}
impl std::error::Error for RecordValidationError {}
impl ObservationRecord {
    pub fn validate(&self) -> Result<(), RecordValidationError> {
        let ts = self.ts.as_bytes();
        if ts.len() != 24
            || ts[19] != b'.'
            || ts[23] != b'Z'
            || !ts[20..23].iter().all(u8::is_ascii_digit)
            || chrono::DateTime::parse_from_rfc3339(&self.ts).is_err()
        {
            return Err(RecordValidationError::Timestamp);
        }
        crate::namespace::validate_namespace(&self.namespace)
            .map_err(RecordValidationError::Namespace)?;
        match (self.phase, self.span.as_ref()) {
            (None, None) | (Some(Phase::Start), Some(_)) => Ok(()),
            (Some(Phase::End), Some(_)) if self.data.is_null() && self.message.is_none() => Ok(()),
            _ => Err(RecordValidationError::Lifecycle),
        }
    }
}

#[cfg(test)]
#[path = "record_certification.rs"]
mod certification;
