use std::{error::Error, fmt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidNamespace(pub String);

impl fmt::Display for InvalidNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid namespace {:?}: expected [a-z][a-z0-9_]* segments separated by ::",
            self.0
        )
    }
}
impl Error for InvalidNamespace {}

pub fn is_normalized_segment(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|c| c.is_ascii_lowercase())
        && bytes.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}

pub fn validate_namespace(value: &str) -> Result<(), InvalidNamespace> {
    if value.split("::").all(is_normalized_segment) {
        Ok(())
    } else {
        Err(InvalidNamespace(value.to_owned()))
    }
}

/// Exact match or descendant, never a partial segment match.
pub fn matches_subtree(root: &str, namespace: &str) -> bool {
    namespace == root
        || namespace
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with("::"))
}

#[cfg(test)]
#[path = "namespace_test.rs"]
mod tests;
