//! Local deterministic simulation runtime.
//!
//! This module is deliberately small, but its executor shape follows madsim's:
//! futures are scheduled as runnable tasks and the ready queue is sampled by a
//! deterministic RNG instead of being driven by a package-level async runtime.

pub mod buggify;
mod executor;
mod rng;
pub mod time;

pub use executor::{yield_now, AbortHandle, Handle, JoinError, JoinHandle, NodeId, Runtime, RuntimeConfig};
pub(crate) use rng::DeterminismLog;
pub use rng::{GlobalRng, Rng};
