//! Local simulation shim for the DST crate.
//!
//! This module is deliberately small, but its executor shape follows madsim's:
//! futures are scheduled as runnable tasks and the ready queue is sampled by a
//! deterministic RNG instead of being driven by a package-level async runtime.

pub(crate) mod commitlog;
mod executor;
mod rng;
mod system_thread;
pub mod time;

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

pub(crate) fn advance_time(duration: Duration) {
    time::advance(duration);
}

pub(crate) fn decision_source(seed: DstSeed) -> DecisionSource {
    DecisionSource::new(seed)
}
