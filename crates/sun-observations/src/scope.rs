use crate::{
    context,
    plan::{next_identity, ResolvedPlan},
    ContextGuard, Observation, Policy, Selection,
};
use parking_lot::Mutex;
use std::{
    any::TypeId,
    collections::HashMap,
    sync::{atomic::AtomicU64, Arc},
};

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

#[derive(Default)]
pub(crate) struct ScopePlans {
    pub plans: HashMap<(u64, TypeId), Arc<ResolvedPlan>>,
    pub counters: HashMap<TypeId, Arc<AtomicU64>>,
}

pub(crate) struct ScopeState {
    pub id: u64,
    pub config: ScopeConfig,
    pub plans: Mutex<ScopePlans>,
    pub capture: Option<Arc<crate::capture::CaptureStorage>>,
}
impl ScopeState {
    pub fn new(config: ScopeConfig) -> Self {
        Self {
            id: next_identity(),
            config,
            plans: Mutex::new(ScopePlans::default()),
            capture: None,
        }
    }
}

pub fn start_scope(config: ScopeConfig) -> ContextGuard {
    let mut scope = ScopeState::new(config);
    scope.capture = context::current_scope().and_then(|parent| parent.capture.clone());
    context::enter_scope(Arc::new(scope))
}

#[cfg(test)]
#[path = "scope_test.rs"]
mod tests;

#[cfg(test)]
#[path = "scope_certification_test.rs"]
mod certification;
