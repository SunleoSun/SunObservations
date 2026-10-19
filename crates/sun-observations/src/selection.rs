use crate::namespace::{is_normalized_segment, matches_subtree, validate_namespace};

/// Empty selection disables everything. Builder methods panic on invalid metadata.
#[derive(Clone, Debug, Default)]
pub struct Selection {
    all: bool,
    namespaces: Vec<String>,
    tags: Vec<String>,
    excluded_namespaces: Vec<String>,
    excluded_tags: Vec<String>,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn activate_all(mut self) -> Self {
        self.all = true;
        self
    }
    pub fn activate_namespace(mut self, namespace: impl Into<String>) -> Self {
        let namespace = namespace.into();
        validate_namespace(&namespace).expect("invalid activation namespace");
        self.namespaces.push(namespace);
        self
    }
    pub fn exclude_namespace(mut self, namespace: impl Into<String>) -> Self {
        let namespace = namespace.into();
        validate_namespace(&namespace).expect("invalid exclusion namespace");
        self.excluded_namespaces.push(namespace);
        self
    }
    pub fn activate_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for tag in tags {
            let tag = tag.into();
            assert!(is_normalized_segment(&tag), "invalid tag");
            self.tags.push(tag);
        }
        self
    }
    pub fn exclude_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for tag in tags {
            let tag = tag.into();
            assert!(is_normalized_segment(&tag), "invalid tag");
            self.excluded_tags.push(tag);
        }
        self
    }
    pub fn activates(&self, namespace: &str, tags: &[&str]) -> bool {
        self.all
            || self
                .namespaces
                .iter()
                .any(|root| matches_subtree(root, namespace))
            || self.tags.iter().any(|tag| tags.contains(&tag.as_str()))
    }
    pub fn excludes(&self, namespace: &str, tags: &[&str]) -> bool {
        self.excluded_namespaces
            .iter()
            .any(|root| matches_subtree(root, namespace))
            || self
                .excluded_tags
                .iter()
                .any(|tag| tags.contains(&tag.as_str()))
    }
    pub fn admits(&self, namespace: &str, tags: &[&str]) -> bool {
        self.activates(namespace, tags) && !self.excludes(namespace, tags)
    }
}

#[cfg(test)]
#[path = "selection_test.rs"]
mod tests;
