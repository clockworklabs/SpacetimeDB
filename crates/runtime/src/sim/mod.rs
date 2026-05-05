//! Local deterministic simulation runtime.
//!
//! This module is deliberately small, but its executor shape follows madsim's:
//! futures are scheduled as runnable tasks and the ready queue is sampled by a
//! deterministic RNG instead of being driven by a package-level async runtime.

mod executor;
mod rng;
mod system_thread;
pub mod time;

use std::time::Duration;

pub use executor::{yield_now, Handle, JoinHandle, NodeId, Runtime};
pub use rng::{DecisionSource, Rng};

pub fn advance_time(duration: Duration) {
    time::advance(duration);
}

pub fn decision_source(seed: u64) -> DecisionSource {
    DecisionSource::new(seed)
}
