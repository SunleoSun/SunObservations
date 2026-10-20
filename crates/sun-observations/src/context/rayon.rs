//! A dedicated Rayon pool bound to the context captured at construction.
//! Every worker installs that context for its lifetime, including nested parallel work.
use super::Context;
use ::rayon::{ThreadPool, ThreadPoolBuildError, ThreadPoolBuilder};

pub struct ContextThreadPool {
    pool: ThreadPool,
    context: Context,
}

impl ContextThreadPool {
    /// Zero uses Rayon's default worker count. The pool remains bound to this scope.
    pub fn new(threads: usize) -> Result<Self, ThreadPoolBuildError> {
        let context = Context::capture();
        let worker_context = context.clone();
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
                builder.spawn(move || context.run(|| worker.run()))?;
                Ok(())
            })
            .build()?;
        Ok(Self { pool, context })
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
#[path = "rayon_test.rs"]
mod tests;

#[cfg(test)]
#[path = "rayon_certification_test.rs"]
mod certification;
