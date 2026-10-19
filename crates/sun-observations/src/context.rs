//! Explicit execution-boundary propagation. A captured context shares scope policy state.
use crate::scope::ScopeState;
use std::{
    cell::RefCell,
    marker::PhantomData,
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

#[cfg(feature = "rayon")]
pub mod rayon;
pub mod thread;

type Frame = (u64, Option<Arc<ScopeState>>);
thread_local! { static STACK: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) }; }
static NEXT_FRAME: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Default)]
pub struct Context {
    scope: Option<Arc<ScopeState>>,
}

impl Context {
    pub fn capture() -> Self {
        Self {
            scope: current_scope(),
        }
    }
    /// Install even an empty context, masking a worker's previous scope until drop.
    pub fn enter(&self) -> ContextGuard {
        let id = NEXT_FRAME.fetch_add(1, Ordering::Relaxed);
        STACK.with(|stack| stack.borrow_mut().push((id, self.scope.clone())));
        ContextGuard {
            id,
            _not_send: PhantomData,
        }
    }
    pub fn run<F, R>(&self, operation: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = self.enter();
        operation()
    }
    pub fn scope_name(&self) -> Option<&str> {
        self.scope.as_ref().map(|s| s.config.name.as_str())
    }
}

/// Thread-bound RAII installation. Removing an older guard never resurrects it later.
#[must_use = "keep this guard alive while the observation context is installed"]
pub struct ContextGuard {
    id: u64,
    _not_send: PhantomData<Rc<()>>,
}
impl Drop for ContextGuard {
    fn drop(&mut self) {
        let _ = STACK.try_with(|stack| stack.borrow_mut().retain(|(id, _)| *id != self.id));
    }
}

pub(crate) fn current_scope() -> Option<Arc<ScopeState>> {
    STACK.with(|stack| stack.borrow().last().and_then(|(_, scope)| scope.clone()))
}
pub(crate) fn enter_scope(scope: Arc<ScopeState>) -> ContextGuard {
    Context { scope: Some(scope) }.enter()
}

#[cfg(test)]
#[path = "context_test.rs"]
mod tests;
