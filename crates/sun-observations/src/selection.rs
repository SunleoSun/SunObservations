use crate::namespace::{prefixes, validate_namespace, validate_tag, InvalidNamespace, InvalidTag};
use std::collections::HashSet;

/// Empty selection disables everything; dynamic identities are validated at construction.
#[derive(Clone, Debug, Default)]
pub struct Selection {
    all: bool,
    namespaces: HashSet<String>,
    tags: HashSet<String>,
    excluded_namespaces: HashSet<String>,
    excluded_tags: HashSet<String>,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn activate_all(mut self) -> Self {
        self.all = true;
        self
    }
    pub fn activate_namespace(
        mut self,
        namespace: impl Into<String>,
    ) -> Result<Self, InvalidNamespace> {
        let namespace = namespace.into();
        validate_namespace(&namespace)?;
        self.namespaces.insert(namespace);
        Ok(self)
    }
    pub fn exclude_namespace(
        mut self,
        namespace: impl Into<String>,
    ) -> Result<Self, InvalidNamespace> {
        let namespace = namespace.into();
        validate_namespace(&namespace)?;
        self.excluded_namespaces.insert(namespace);
        Ok(self)
    }
    pub fn activate_tags<I, S>(mut self, tags: I) -> Result<Self, InvalidTag>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for tag in tags {
            let tag = tag.into();
            validate_tag(&tag)?;
            self.tags.insert(tag);
        }
        Ok(self)
    }
    pub fn exclude_tags<I, S>(mut self, tags: I) -> Result<Self, InvalidTag>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for tag in tags {
            let tag = tag.into();
            validate_tag(&tag)?;
            self.excluded_tags.insert(tag);
        }
        Ok(self)
    }
    pub fn activates(&self, namespace: &str, tags: &[&str]) -> bool {
        self.all
            || prefixes(namespace).any(|p| self.namespaces.contains(p))
            || tags.iter().any(|tag| self.tags.contains(*tag))
    }
    pub fn excludes(&self, namespace: &str, tags: &[&str]) -> bool {
        prefixes(namespace).any(|p| self.excluded_namespaces.contains(p))
            || tags.iter().any(|tag| self.excluded_tags.contains(*tag))
    }
    pub fn admits(&self, namespace: &str, tags: &[&str]) -> bool {
        self.activates(namespace, tags) && !self.excludes(namespace, tags)
    }
}

#[cfg(test)]
#[path = "selection_certification.rs"]
mod certification;
