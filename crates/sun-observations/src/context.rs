//! Explicit boundary propagation. Scope plans/counters are shared; TLS holds four fast references and a bounded weak overflow.
use crate::{engine::ObservationEngine, plan::ResolvedPlan, scope::ScopeState, Observation};
use std::{
    any::TypeId,
    cell::RefCell,
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Weak},
};

pub mod rayon;
pub mod thread;

const PLAN_CACHE_CAPACITY: usize = 4;
const WEAK_CACHE_CAPACITY: usize = 28;

type Frame = (u64, Option<Arc<ScopeState>>);
pub(crate) type CachedPlan = Rc<Arc<ResolvedPlan>>;
struct Entry<P> {
    engine: u64,
    scope: u64,
    generation: u64,
    ty: TypeId,
    plan: P,
}
struct Local {
    stack: Vec<Frame>,
    cache: [Option<Entry<CachedPlan>>; PLAN_CACHE_CAPACITY],
    // Overflow must not extend detached sink or capture ownership beyond the four strong slots.
    overflow: std::collections::VecDeque<Entry<Weak<ResolvedPlan>>>,
}
thread_local! {
    static LOCAL: RefCell<Local> = const { RefCell::new(Local { stack: Vec::new(), cache: [const { None }; PLAN_CACHE_CAPACITY], overflow: std::collections::VecDeque::new() }) };
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
            {
                local.overflow.clear();
                std::mem::replace(&mut local.cache, [const { None }; PLAN_CACHE_CAPACITY])
            }
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
            (frame, {
                local.overflow.clear();
                std::mem::replace(&mut local.cache, [const { None }; PLAN_CACHE_CAPACITY])
            })
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
    let generation = engine.generation();
    let cached = LOCAL.with(|local| {
        local.borrow().overflow.iter().find_map(|entry| {
            (entry.engine == engine.id
                && entry.scope == scope.id
                && entry.generation == generation
                && entry.ty == TypeId::of::<T>())
            .then(|| entry.plan.upgrade())
            .flatten()
        })
    });
    if let Some(plan) = cached {
        return Rc::new(plan);
    }
    let plan = Rc::new(engine.resolve::<T>(scope));
    // Retiring a plan can drop a user sink. Do not hold a TLS borrow across that callback.
    let retired = LOCAL.with(|local| {
        let mut local = local.borrow_mut();
        local.cache.rotate_right(1);
        let retired = local.cache[0].replace(Entry {
            engine: engine.id,
            scope: scope.id,
            generation: plan.generation,
            ty: TypeId::of::<T>(),
            plan: plan.clone(),
        });
        if let Some(entry) = &retired {
            if local.overflow.len() == WEAK_CACHE_CAPACITY {
                local.overflow.pop_back();
            }
            local.overflow.push_front(Entry {
                engine: entry.engine,
                scope: entry.scope,
                generation: entry.generation,
                ty: entry.ty,
                plan: Arc::downgrade(&entry.plan),
            });
        }
        retired
    });
    drop(retired);
    plan
}

pub(crate) fn forget_engine(engine: u64) {
    let retired = LOCAL.try_with(|local| {
        local.try_borrow_mut().ok().map(|mut local| {
            local.overflow.retain(|entry| entry.engine != engine);
            std::array::from_fn::<_, PLAN_CACHE_CAPACITY, _>(|index| {
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
