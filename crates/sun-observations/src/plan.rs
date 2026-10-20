//! Resolution is cold; the steady-state path only reads a TLS entry and an atomic generation.
use crate::{Policy, Sink};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

pub(crate) struct ResolvedPlan {
    pub generation: u64,
    pub active: bool,
    pub policy: Policy,
    pub counter: Option<Arc<AtomicU64>>,
    pub scope: Option<String>,
    pub sinks: Arc<[Arc<dyn Sink>]>,
    pub needs_render_text: bool,
}
impl ResolvedPlan {
    #[inline]
    pub fn admit(&self) -> bool {
        if !self.active {
            return false;
        }
        match self.policy {
            Policy::Always => true,
            Policy::Never => false,
            policy => {
                policy.admit_atomic(self.counter.as_ref().expect("counting plan has counter"))
            }
        }
    }
}

pub(crate) fn next_identity() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
#[path = "plan_test.rs"]
mod tests;

#[cfg(test)]
#[path = "plan_certification_test.rs"]
mod certification;
