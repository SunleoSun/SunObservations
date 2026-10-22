//! A dedicated Rayon pool bound to the context captured at construction.
//! Every worker installs that context for its lifetime, including nested parallel work.
use super::Context;
use ::rayon::{ThreadPool, ThreadPoolBuildError, ThreadPoolBuilder};
use parking_lot::Mutex;
use std::{sync::Arc, thread::JoinHandle};

/// Worker ownership follows pool termination so propagated context is released before owner Drop returns.
#[derive(Default)]
struct Workers(Arc<Mutex<Vec<JoinHandle<()>>>>);
impl Drop for Workers {
    fn drop(&mut self) {
        let handles = std::mem::take(&mut *self.0.lock());
        // A last owner dropped by its own detached job cannot join the executing pool.
        // Its capture remains incomplete until those escaped worker contexts are released.
        if handles
            .iter()
            .any(|h| h.thread().id() == std::thread::current().id())
        {
            return;
        }
        for handle in handles {
            let _ = handle.join();
        }
    }
}

/// A context-bound dedicated pool whose owner joins terminated workers before releasing context.
pub struct ContextThreadPool {
    pool: ThreadPool,
    _workers: Workers,
    context: Context,
}

impl ContextThreadPool {
    /// Zero uses Rayon's default worker count. The pool remains bound to this scope.
    pub fn new(threads: usize) -> Result<Self, ThreadPoolBuildError> {
        let context = Context::capture();
        let worker_context = context.clone();
        let workers = Workers::default();
        let handles = workers.0.clone();
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .spawn_handler(move |worker| {
                let context = worker_context.clone();
                let mut builder = std::thread::Builder::new();
                if let Some(name) = worker.name() {
                    builder = builder.name(name.to_owned());
                }
                if let Some(size) = worker.stack_size() {
                    builder = builder.stack_size(size);
                }
                let handle = builder.spawn(move || context.run(|| worker.run()))?;
                handles.lock().push(handle);
                Ok(())
            })
            .build()?;
        Ok(Self {
            pool,
            _workers: workers,
            context,
        })
    }

    pub fn install<F, R>(&self, operation: F) -> R
    where
        F: FnOnce() -> R + Send,
        R: Send,
    {
        self.pool.install(|| self.context.run(operation))
    }
}

#[cfg(test)]
#[path = "rayon_certification.rs"]
mod certification;
