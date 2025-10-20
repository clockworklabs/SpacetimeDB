#![allow(dead_code)]

//! Provides budget, energy, timeout, and long-running logging facilities.
//!
//! These are all driven by [`with_timeout_and_cb_every`] for V8 modules
//! as V8 has no native notion of gas/fuel,
//! so we have to invent one using time and timeouts.

use super::env_on_isolate;
use crate::host::wasm_common::module_host_actor::EnergyStats;
use crate::host::wasmtime::{epoch_ticker, ticks_in_duration};
use core::ptr;
use core::sync::atomic::Ordering;
use core::time::Duration;
use core::{ffi::c_void, sync::atomic::AtomicBool};
use spacetimedb_client_api_messages::energy::ReducerBudget;
use std::sync::Arc;
use v8::{Isolate, IsolateHandle};

/// Runs `logic` concurrently wth a thread that will terminate JS execution
/// when `budget` has been used up.
///
/// Every `callback_every` ticks, `callback` is called.
pub(super) fn with_timeout_and_cb_every<R>(
    _handle: IsolateHandle,
    _callback_every: u64,
    _callback: InterruptCallback,
    _budget: ReducerBudget,
    logic: impl FnOnce() -> R,
) -> R {
    // Start the concurrent thread.
    // TODO(v8): This currently leads to UB as there are bugs in th v8 crate.
    //let timeout_thread_cancel_flag = run_timeout_and_cb_every(handle, callback_every, callback, budget);

    #[allow(clippy::let_and_return)]
    let ret = logic();

    // Cancel the execution timeout in `run_timeout_and_cb_every`.
    //timeout_thread_cancel_flag.store(true, Ordering::Relaxed);

    ret
}

/// A callback passed to [`IsolateHandle::request_interrupt`].
pub(super) type InterruptCallback = extern "C" fn(&mut Isolate, *mut c_void);

/// An [`InterruptCallback`] used by `call_reducer`,
/// and called by a thread separate to V8 execution
/// every [`EPOCH_TICKS_PER_SECOND`] ticks (~every 1 second)
/// to log that the reducer is still running.
pub(super) extern "C" fn cb_log_long_running(isolate: &mut Isolate, _: *mut c_void) {
    let Some(env) = env_on_isolate(isolate) else {
        // All we can do is log something.
        tracing::error!("`JsInstanceEnv` not set");
        return;
    };
    let database = env.instance_env.replica_ctx.database_identity;
    let reducer = env.reducer_name();
    let dur = env.reducer_start().elapsed();
    tracing::warn!(reducer, ?database, "JavaScript has been running for {dur:?}");
}

/// An [`InterruptCallback`] that does nothing.
pub(super) extern "C" fn cb_noop(_: &mut Isolate, _: *mut c_void) {}

/// Spawns a thread that will terminate execution
/// when `budget` has been used up.
///
/// Every `callback_every` ticks, `callback` is called.
fn run_timeout_and_cb_every(
    handle: IsolateHandle,
    callback_every: u64,
    callback: InterruptCallback,
    budget: ReducerBudget,
) -> Arc<AtomicBool> {
    // When `execution_done_flag` is set, the ticker thread will stop.
    let execution_done_flag = Arc::new(AtomicBool::new(false));
    let execution_done_flag2 = execution_done_flag.clone();

    let timeout = budget_to_duration(budget);
    let max_ticks = ticks_in_duration(timeout);

    let mut num_ticks = 0;
    epoch_ticker(move || {
        // Check if execution completed.
        if execution_done_flag2.load(Ordering::Relaxed) {
            return None;
        }

        // We've reached the number of ticks to call `callback`.
        if num_ticks % callback_every == 0 && handle.request_interrupt(callback, ptr::null_mut()) {
            return None;
        }

        if num_ticks == max_ticks {
            // Execution still ongoing while budget has been exhausted.
            // Terminate V8 execution.
            // This implements "gas" for v8.
            handle.terminate_execution();
        }

        num_ticks += 1;
        Some(())
    });

    execution_done_flag
}

/// Converts a [`ReducerBudget`] to a [`Duration`].
fn budget_to_duration(_budget: ReducerBudget) -> Duration {
    // TODO(v8): This is fake logic that allows a maximum timeout.
    // Replace with sensible math.
    Duration::MAX
}

/// Returns [`EnergyStats`] for a reducer given its `budget`
/// and the `duration` it took to execute.
pub(super) fn energy_from_elapsed(budget: ReducerBudget, duration: Duration) -> EnergyStats {
    let used = duration_to_budget(duration);
    let remaining = budget - used;
    EnergyStats { budget, remaining }
}

/// Converts a [`Duration`] to a [`ReducerBudget`].
fn duration_to_budget(_duration: Duration) -> ReducerBudget {
    // TODO(v8): This is fake logic that allows minimum energy usage.
    // Replace with sensible math.
    ReducerBudget::ZERO
}
