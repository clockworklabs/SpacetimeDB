//! Std-hosted entry points for running the deterministic simulator in tests.
//!
//! The portable simulator lives in [`crate::sim`]. This module is deliberately
//! host-specific: it installs thread-local context while a simulation is
//! running, checks determinism by replaying a seed in fresh OS threads, and
//! intercepts a few libc calls so std code cannot silently escape determinism.

#![allow(clippy::disallowed_macros)]

use alloc::boxed::Box;
use core::{cell::Cell, future::Future};

use crate::sim;

// Public entry points.

/// Run a future to completion with std-hosted determinism guards installed.
///
/// This wraps [`sim::Runtime::block_on`] and is the normal entry point for DST
/// tests that execute inside a hosted process. While the future runs, this
/// marks the thread as inside simulation so OS thread spawns can be rejected.
pub fn block_on<F: Future>(runtime: &mut sim::Runtime, future: F) -> F::Output {
    let _system_thread_context = enter_simulation_thread();
    runtime.block_on(future)
}

/// Run the same future factory twice and assert that both runs consume the same
/// deterministic RNG/scheduler trace.
///
/// Each pass runs on a fresh OS thread so thread-local std state is not shared
/// between the recording and replay passes.
pub fn check_determinism<M, F>(seed: u64, make_future: M) -> F::Output
where
    M: Fn() -> F + Clone + Send + 'static,
    F: Future + 'static,
    F::Output: Send + 'static,
{
    let first = make_future.clone();
    let log = std::thread::spawn(move || {
        let mut runtime = sim::Runtime::new(seed);
        runtime.enable_determinism_log();
        block_on(&mut runtime, first());
        runtime
            .take_determinism_log()
            .expect("determinism log should be enabled")
    })
    .join()
    .map_err(|payload| panic_with_seed(seed, payload))
    .unwrap();

    std::thread::spawn(move || {
        let mut runtime = sim::Runtime::new(seed);
        runtime.enable_determinism_check(log);
        let output = block_on(&mut runtime, make_future());
        runtime.finish_determinism_check().unwrap_or_else(|err| panic!("{err}"));
        output
    })
    .join()
    .map_err(|payload| panic_with_seed(seed, payload))
    .unwrap()
}

fn panic_with_seed(seed: u64, payload: Box<dyn core::any::Any + Send>) -> ! {
    eprintln!("note: run with --seed {seed} to reproduce this error");
    std::panic::resume_unwind(payload);
}

// Simulation thread context.

// Ambient state used only while `sim_std::block_on` is driving a simulation.
//
// The simulator itself stays explicit-handle based. This thread-local only
// marks whether the current OS thread is owned by a running simulation so
// host thread creation can be rejected.
thread_local! {
    // Marks the current OS thread as simulation-owned so thread creation hooks
    // can reject accidental escapes to the host scheduler.
    static IN_SIMULATION: Cell<bool> = const { Cell::new(false) };
}

struct SimulationThreadGuard {
    previous: bool,
}

fn enter_simulation_thread() -> SimulationThreadGuard {
    crate::hooks::install();
    let previous = IN_SIMULATION.with(|state| state.replace(true));
    SimulationThreadGuard { previous }
}

pub(crate) fn in_simulation() -> bool {
    IN_SIMULATION.with(Cell::get)
}

impl Drop for SimulationThreadGuard {
    fn drop(&mut self) {
        IN_SIMULATION.with(|state| {
            state.set(self.previous);
        });
    }
}


