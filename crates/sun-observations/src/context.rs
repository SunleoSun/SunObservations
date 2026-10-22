//! Explicit boundary propagation. Scope plans/counters are shared; TLS only holds four fast references.
use crate::{engine::ObservationEngine, plan::ResolvedPlan, scope::ScopeState, Observation};
use std::{any::TypeId, cell::RefCell, marker::PhantomData, rc::Rc, sync::Arc};

pub mod rayon;
pub mod thread;

type Frame = (u64, Option<Arc<ScopeState>>);
pub(crate) type CachedPlan = Rc<Arc<ResolvedPlan>>;
struct Entry {
    engine: u64,
    scope: u64,
    generation: u64,
    ty: TypeId,
    plan: CachedPlan,
}
struct Local {
    stack: Vec<Frame>,
    cache: [Option<Entry>; 4],
}
thread_local! {
    static LOCAL: RefCell<Local> = const { RefCell::new(Local { stack: Vec::new(), cache: [const { None }; 4] }) };
}

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
    pub fn enter(&self) -> ContextGuard {
        let id = crate::plan::next_identity();
        let retired = LOCAL.with(|local| {
            let mut local = local.borrow_mut();
            local.stack.push((id, self.scope.clone()));
            std::mem::replace(&mut local.cache, [const { None }; 4])
        });
        drop(retired);
        ContextGuard {
            id,
            _not_send: PhantomData,
        }
    }
    pub fn run<F: FnOnce() -> R, R>(&self, operation: F) -> R {
        let _guard = self.enter();
        operation()
    }
    pub fn scope_name(&self) -> Option<&str> {
        self.scope.as_ref().map(|s| s.config.name.as_str())
    }
}

#[must_use = "keep this guard alive while the observation context is installed"]
pub struct ContextGuard {
    id: u64,
    _not_send: PhantomData<Rc<()>>,
}
impl Drop for ContextGuard {
    fn drop(&mut self) {
        let retired = LOCAL.try_with(|local| {
            let mut local = local.borrow_mut();
            let frame = local
                .stack
                .iter()
                .position(|(id, _)| *id == self.id)
                .map(|index| local.stack.remove(index));
            (
                frame,
                std::mem::replace(&mut local.cache, [const { None }; 4]),
            )
        });
        drop(retired);
    }
}
pub(crate) fn current_scope() -> Option<Arc<ScopeState>> {
    LOCAL.with(|local| {
        local
            .borrow()
            .stack
            .last()
            .and_then(|(_, scope)| scope.clone())
    })
}
pub(crate) fn enter_scope(scope: Arc<ScopeState>) -> ContextGuard {
    Context { scope: Some(scope) }.enter()
}

#[inline]
pub(crate) fn cached_plan<T: Observation>(engine: &ObservationEngine) -> CachedPlan {
    let generation = engine.generation();
    let lookup = LOCAL.with(|local| {
        let local = local.borrow();
        let scope = local.stack.last().and_then(|(_, scope)| scope.as_ref());
        let scope_id = scope.map_or(engine.unscoped.id, |scope| scope.id);
        let ty = TypeId::of::<T>();
        for entry in local.cache.iter().flatten() {
            if entry.engine == engine.id
                && entry.scope == scope_id
                && entry.generation == generation
                && entry.ty == ty
            {
                return Ok(entry.plan.clone());
            }
        }
        Err(scope.cloned())
    });
    match lookup {
        Ok(plan) => plan,
        Err(scope) => resolve_plan::<T>(engine, scope),
    }
}

#[cold]
fn resolve_plan<T: Observation>(
    engine: &ObservationEngine,
    scope: Option<Arc<ScopeState>>,
) -> CachedPlan {
    let scope = scope.as_deref().unwrap_or(&engine.unscoped);
    let plan = Rc::new(engine.resolve::<T>(scope));
    // Retiring a plan can drop a user sink. Do not hold a TLS borrow across that callback.
    let retired = LOCAL.with(|local| {
        let mut local = local.borrow_mut();
        local.cache.rotate_right(1);
        local.cache[0].replace(Entry {
            engine: engine.id,
            scope: scope.id,
            generation: plan.generation,
            ty: TypeId::of::<T>(),
            plan: plan.clone(),
        })
    });
    drop(retired);
    plan
}

pub(crate) fn forget_engine(engine: u64) {
    let retired = LOCAL.try_with(|local| {
        local.try_borrow_mut().ok().map(|mut local| {
            std::array::from_fn::<_, 4, _>(|index| {
                if local.cache[index]
                    .as_ref()
                    .is_some_and(|entry| entry.engine == engine)
                {
                    local.cache[index].take()
                } else {
                    None
                }
            })
        })
    });
    drop(retired);
}

#[cfg(test)]
#[path = "context_certification.rs"]
mod certification;
