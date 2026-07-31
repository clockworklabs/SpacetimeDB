//! Std facade for the portable deterministic simulation runtime.
//!
//! The simulator core lives in `spacetimedb-runtime-core` and stays `no_std`.
//! This module preserves the `spacetimedb_runtime::sim` API surface while
//! adding the hosted context behavior that belongs in `spacetimedb-runtime`.

use core::{
    future::Future,
    ops::{Deref, DerefMut},
    time::Duration,
};

pub use spacetimedb_runtime_core::sim::{
    buggify, time, yield_now, AbortHandle, GlobalRng, Handle, JoinError, JoinHandle, Node, NodeBuilder, NodeId, Rng,
    RuntimeConfig,
};

/// Hosted wrapper around the portable simulation runtime.
pub struct Runtime {
    inner: spacetimedb_runtime_core::sim::Runtime,
}

impl Runtime {
    /// Create a simulation runtime seeded for deterministic scheduling and RNG.
    pub fn new(seed: u64) -> Self {
        Self::with_config(RuntimeConfig::new(seed))
    }

    /// Create a simulation runtime from an explicit runtime configuration.
    pub fn with_config(config: RuntimeConfig) -> Self {
        Self {
            inner: spacetimedb_runtime_core::sim::Runtime::with_config(config),
        }
    }

    /// Drive a top-level future to completion with hosted simulation context installed.
    pub fn block_on<F: Future>(&mut self, future: F) -> F::Output {
        let _guard = crate::sim_std::enter(self.handle());
        self.inner.block_on(future)
    }

    /// Return the amount of virtual time elapsed in this runtime.
    pub fn elapsed(&self) -> Duration {
        self.inner.elapsed()
    }

    /// Get a cloneable handle for spawning tasks and accessing runtime services.
    pub fn handle(&self) -> Handle {
        self.inner.handle()
    }

    /// Create a new simulated node.
    pub fn create_node(&self) -> NodeBuilder {
        self.inner.create_node()
    }

    /// Pause scheduling for a node.
    pub fn pause(&self, node: NodeId) {
        self.inner.pause(node);
    }

    /// Resume scheduling for a previously paused node.
    pub fn resume(&self, node: NodeId) {
        self.inner.resume(node);
    }

    /// Spawn a `Send` future onto a specific simulated node.
    pub fn spawn_on<F>(&self, node: NodeId, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.inner.spawn_on(node, future)
    }

    pub fn enable_buggify(&self) {
        self.inner.enable_buggify();
    }

    /// Disable probabilistic fault injection for this runtime.
    pub fn disable_buggify(&self) {
        self.inner.disable_buggify();
    }

    /// Return whether buggify is enabled for this runtime.
    pub fn is_buggify_enabled(&self) -> bool {
        self.inner.is_buggify_enabled()
    }

    /// Sample the default runtime buggify probability.
    pub fn buggify(&self) -> bool {
        self.inner.buggify()
    }

    /// Sample a caller-provided runtime buggify probability.
    pub fn buggify_with_prob(&self, probability: f64) -> bool {
        self.inner.buggify_with_prob(probability)
    }

    pub(crate) fn enable_determinism_log(&self) {
        self.inner.enable_determinism_log();
    }

    pub(crate) fn enable_determinism_check(&self, log: spacetimedb_runtime_core::sim::DeterminismLog) {
        self.inner.enable_determinism_check(log);
    }

    pub(crate) fn take_determinism_log(&self) -> Option<spacetimedb_runtime_core::sim::DeterminismLog> {
        self.inner.take_determinism_log()
    }

    pub(crate) fn finish_determinism_check(&self) -> Result<(), alloc::string::String> {
        self.inner.finish_determinism_check()
    }
}

impl Deref for Runtime {
    type Target = spacetimedb_runtime_core::sim::Runtime;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Runtime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
