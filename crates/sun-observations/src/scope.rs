use crate::{context, ContextGuard, Observation, Policy, Selection};
use parking_lot::Mutex;
use std::{any::TypeId, collections::HashMap, sync::Arc};

/// One execution identity, not a verbosity category. Each start creates fresh counters.
#[derive(Clone, Debug)]
pub struct ScopeConfig {
    pub(crate) name: String,
    pub(crate) selection: Selection,
    pub(crate) policies: HashMap<TypeId, Policy>,
}

impl ScopeConfig {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            selection: Selection::new(),
            policies: HashMap::new(),
        }
    }
    pub fn selection(mut self, selection: Selection) -> Self {
        self.selection = selection;
        self
    }
    pub fn activate_all(mut self) -> Self {
        self.selection = self.selection.activate_all();
        self
    }
    pub fn activate_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.selection = self.selection.activate_namespace(namespace);
        self
    }
    pub fn exclude_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.selection = self.selection.exclude_namespace(namespace);
        self
    }
    pub fn activate_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.selection = self.selection.activate_tags(tags);
        self
    }
    pub fn exclude_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.selection = self.selection.exclude_tags(tags);
        self
    }
    pub fn override_policy<T: Observation>(mut self, policy: Policy) -> Self {
        self.policies.insert(TypeId::of::<T>(), policy);
        self
    }
}

pub(crate) struct ScopeState {
    pub config: ScopeConfig,
    counters: Mutex<HashMap<TypeId, u64>>,
}

impl ScopeState {
    pub fn new(config: ScopeConfig) -> Self {
        Self {
            config,
            counters: Mutex::new(HashMap::new()),
        }
    }
    pub fn admit<T: Observation>(&self) -> bool {
        let policy = self
            .config
            .policies
            .get(&TypeId::of::<T>())
            .copied()
            .unwrap_or(Policy::Always);
        match policy {
            Policy::Always => true,
            Policy::Never => false,
            _ => policy.admit(self.counters.lock().entry(TypeId::of::<T>()).or_default()),
        }
    }
}

pub fn start_scope(config: ScopeConfig) -> ContextGuard {
    context::enter_scope(Arc::new(ScopeState::new(config)))
}

#[cfg(test)]
#[path = "scope_test.rs"]
mod tests;
