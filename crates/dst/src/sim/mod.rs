//! Local simulation shim for the DST crate.
//!
//! This module is deliberately small, but its executor shape follows madsim's:
//! futures are scheduled as runnable tasks and the ready queue is sampled by a
//! deterministic RNG instead of being driven by a package-level async runtime.

pub(crate) mod commitlog;
pub mod time;

use std::{future::Future, time::Duration};

pub use spacetimedb_runtime::sim::{yield_now, DecisionSource, Handle, JoinHandle, NodeId, Rng};

use crate::seed::DstSeed;

/// DST-facing wrapper that keeps the top-level seed type local to this crate.
pub struct Runtime {
    inner: spacetimedb_runtime::sim::Runtime,
}

impl Runtime {
    pub fn new(seed: DstSeed) -> anyhow::Result<Self> {
        Ok(Self {
            inner: spacetimedb_runtime::sim::Runtime::new(seed.0)?,
        })
    }

    pub fn block_on<F: Future>(&mut self, future: F) -> F::Output {
        self.inner.block_on(future)
    }

    pub fn elapsed(&self) -> Duration {
        self.inner.elapsed()
    }

    pub fn handle(&self) -> Handle {
        self.inner.handle()
    }

    pub fn create_node(&self) -> NodeId {
        self.inner.create_node()
    }

    pub fn pause(&self, node: NodeId) {
        self.inner.pause(node);
    }

    pub fn resume(&self, node: NodeId) {
        self.inner.resume(node);
    }

    pub fn spawn_on<F>(&self, node: NodeId, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.inner.spawn_on(node, future)
    }

    pub fn check_determinism<F>(seed: DstSeed, make_future: fn() -> F) -> F::Output
    where
        F: Future + 'static,
        F::Output: Send + 'static,
    {
        spacetimedb_runtime::sim::Runtime::check_determinism(seed.0, make_future)
    }

    pub fn check_determinism_with<M, F>(seed: DstSeed, make_future: M) -> F::Output
    where
        M: Fn() -> F + Clone + Send + 'static,
        F: Future + 'static,
        F::Output: Send + 'static,
    {
        spacetimedb_runtime::sim::Runtime::check_determinism_with(seed.0, make_future)
    }
}

pub(crate) fn advance_time(duration: Duration) {
    time::advance(duration);
}

pub(crate) fn decision_source(seed: DstSeed) -> DecisionSource {
    DecisionSource::new(seed.0)
}
