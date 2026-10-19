use serde::{Deserialize, Serialize};
use std::fmt;

/// Severity only; selection and policies control observation detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationLevel {
    INFO,
    WARN,
    ERROR,
}

impl fmt::Display for ObservationLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::INFO => "INFO",
            Self::WARN => "WARN",
            Self::ERROR => "ERROR",
        })
    }
}

#[cfg(test)]
#[path = "level_test.rs"]
mod tests;
