//! OS-thread boundary that captures the caller's current observation context.
use super::Context;
use std::thread::{self, JoinHandle};

pub fn spawn<F, R>(operation: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let context = Context::capture();
    thread::spawn(move || context.run(operation))
}

#[cfg(test)]
#[path = "thread_test.rs"]
mod tests;

#[cfg(test)]
#[path = "thread_certification_test.rs"]
mod certification;
