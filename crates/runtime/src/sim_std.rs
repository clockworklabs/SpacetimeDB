//! Std-hosted entry points for running the deterministic simulator in tests.
//!
//! The portable simulator lives in [`crate::sim`]. This module is deliberately
//! host-specific: it installs thread-local context while a simulation is
//! running, checks determinism by replaying a seed in fresh OS threads, and
//! intercepts a few libc calls so std code cannot silently escape determinism.

#![allow(clippy::disallowed_macros)]

use alloc::boxed::Box;
use core::{cell::Cell, future::Future};
use std::sync::OnceLock;

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
    let previous = IN_SIMULATION.with(|state| state.replace(true));
    SimulationThreadGuard { previous }
}

fn in_simulation() -> bool {
    IN_SIMULATION.with(Cell::get)
}

impl Drop for SimulationThreadGuard {
    fn drop(&mut self) {
        IN_SIMULATION.with(|state| {
            state.set(self.previous);
        });
    }
}

// Thread hook.

// Hook Unix thread creation by interposing `pthread_attr_init`.
//
// `std::thread::Builder::spawn` initializes pthread attributes before creating
// the thread. Returning an error here while simulation is active makes hidden
// OS thread creation fail early, before host scheduling can affect replay.
// Outside simulation, this delegates to the real libc symbol through `RTLD_NEXT`.
#[cfg(unix)]
#[unsafe(no_mangle)]
#[inline(never)]
unsafe extern "C" fn pthread_attr_init(attr: *mut libc::pthread_attr_t) -> libc::c_int {
    // std::thread enters libc through pthread_attr_init on Unix. Refusing that
    // call while in simulation keeps hidden OS scheduling out of DST.
    if in_simulation() {
        eprintln!("attempt to spawn a system thread in simulation.");
        eprintln!("note: use simulator tasks instead.");
        return -1;
    }

    type PthreadAttrInit = unsafe extern "C" fn(*mut libc::pthread_attr_t) -> libc::c_int;
    static PTHREAD_ATTR_INIT: OnceLock<PthreadAttrInit> = OnceLock::new();
    let original = PTHREAD_ATTR_INIT.get_or_init(|| unsafe {
        // `RTLD_NEXT` skips this interposed function and finds the libc
        // implementation that would have been called without the simulator.
        let ptr = libc::dlsym(libc::RTLD_NEXT, c"pthread_attr_init".as_ptr().cast());
        assert!(!ptr.is_null(), "failed to resolve original pthread_attr_init");
        std::mem::transmute(ptr)
    });
    unsafe { original(attr) }
}

// Randomness syscall hooks.

// Hook OS randomness by interposing `getrandom`.
//
// This crate no longer tries to make host randomness deterministic. Any such
// request is surfaced with a warning and then delegated to the host OS.
#[unsafe(no_mangle)]
#[inline(never)]
unsafe extern "C" fn getrandom(buf: *mut u8, buflen: usize, flags: u32) -> isize {
    if in_simulation() {
        eprintln!("warning: randomness requested; delegating to host OS");
        eprintln!("{}", std::backtrace::Backtrace::force_capture());
    }
    unsafe { real_getrandom()(buf, buflen, flags) }
}

#[cfg(target_os = "linux")]
fn real_getrandom() -> unsafe extern "C" fn(*mut u8, usize, u32) -> isize {
    type GetrandomFn = unsafe extern "C" fn(*mut u8, usize, u32) -> isize;
    static GETRANDOM: OnceLock<GetrandomFn> = OnceLock::new();
    *GETRANDOM.get_or_init(|| unsafe {
        let ptr = libc::dlsym(libc::RTLD_NEXT, c"getrandom".as_ptr().cast());
        assert!(!ptr.is_null(), "failed to resolve original getrandom");
        std::mem::transmute(ptr)
    })
}

#[cfg(not(target_os = "linux"))]
fn real_getrandom() -> unsafe extern "C" fn(*mut u8, usize, u32) -> isize {
    compile_error!("unsupported OS for DST getrandom override");
}

// Hook `getentropy` and route it through the same deterministic path as
// `getrandom`.
//
// The 256-byte limit is part of the getentropy contract. Keeping this wrapper
// small means all entropy decisions stay centralized in `getrandom`.
#[unsafe(no_mangle)]
#[inline(never)]
unsafe extern "C" fn getentropy(buf: *mut u8, buflen: usize) -> i32 {
    if buflen > 256 {
        return -1;
    }
    match unsafe { getrandom(buf, buflen, 0) } {
        -1 => -1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use crate::sim;

    use super::getentropy;

    #[test]
    #[cfg(unix)]
    fn runtime_forbids_system_thread_spawn() {
        let mut runtime = sim::Runtime::new(200);
        super::block_on(&mut runtime, async {
            let result = std::panic::catch_unwind(|| std::thread::Builder::new().spawn(|| {}));
            assert!(result.is_err());
        });
    }

    #[test]
    fn getentropy_delegates_to_host_randomness_outside_simulation() {
        let mut actual = [0u8; 24];
        unsafe {
            assert_eq!(getentropy(actual.as_mut_ptr(), actual.len()), 0);
        }
    }
}
