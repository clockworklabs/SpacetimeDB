//! Local simulation shim for the DST crate.
//!
//! This module is deliberately small, but its executor shape follows madsim's:
//! futures are scheduled as runnable tasks and the ready queue is sampled by a
//! deterministic RNG instead of being driven by a package-level async runtime.

mod executor;
mod rng;

use std::time::Duration;

pub use executor::{yield_now, Handle, JoinHandle, NodeId, Runtime};
pub use rng::Rng;

use crate::seed::DstSeed;

pub(crate) use rng::DecisionSource;

pub(crate) type RuntimeHandle = spacetimedb_core::runtime::Handle;
pub(crate) type RuntimeGuard = spacetimedb_core::runtime::Runtime;

pub(crate) fn current_handle_or_new_runtime() -> anyhow::Result<(RuntimeHandle, Option<RuntimeGuard>)> {
    spacetimedb_core::runtime::current_handle_or_new_runtime()
}

pub(crate) fn advance_time(_duration: Duration) {
    // This is a hook, not wall-clock sleep. A future simulator layer can advance
    // virtual time here while keeping targets on the same API.
}

pub(crate) fn decision_source(seed: DstSeed) -> DecisionSource {
    DecisionSource::new(seed)
}
