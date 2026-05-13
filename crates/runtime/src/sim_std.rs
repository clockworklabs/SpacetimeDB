//! Std-hosted entry points for running the deterministic simulator in tests.
//!
//! The portable simulator lives in [`crate::sim`]. This module is deliberately
//! host-specific: it installs thread-local context while a simulation is
//! running, checks determinism by replaying a seed in fresh OS threads, and
//! intercepts a few libc calls so std code cannot silently escape determinism.

use alloc::boxed::Box;
use core::{
    cell::{Cell, RefCell},
    future::Future,
    ptr,
};
use std::sync::OnceLock;

use crate::sim;

// Public entry points.

/// Return the generic runtime facade for the current simulation thread.
///
/// Prefer passing explicit [`sim::Handle`] values in simulation code. This is a
/// hosted convenience for code paths that already accept [`crate::Runtime`].
pub fn simulation_current() -> crate::Runtime {
    crate::Runtime::simulation(current_handle().expect("simulation runtime is not active on this thread"))
}

/// Run a future to completion with std-hosted determinism guards installed.
///
/// This wraps [`sim::Runtime::block_on`] and is the normal entry point for DST
/// tests that execute inside a hosted process. While the future runs, this
/// function exposes the current simulation handle, routes std randomness
/// through the simulation RNG, and marks the thread as inside simulation so OS
/// thread spawns can be rejected.
pub fn block_on<F: Future>(runtime: &mut sim::Runtime, future: F) -> F::Output {
    let _handle_context = enter_handle_context(runtime.handle());
    let _system_thread_context = enter_simulation_thread();
    let _rng_context = enter_rng_context(runtime.rng());
    ensure_rng_hooks_linked();
    runtime.block_on(future)
}

/// Return the current simulation handle if this thread is inside [`block_on`].
///
/// This is intentionally the only ambient context accessor. Time, buggify, and
/// task APIs should be reached through the returned handle or through explicit
/// handles passed by the caller.
pub fn current_handle() -> Option<sim::Handle> {
    CURRENT_HANDLE.with(|handle| handle.borrow().clone())
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
// The simulator itself stays explicit-handle based. These thread-locals exist
// because std and libc hooks do not accept a `sim::Handle` parameter, and
// because a few hosted test helpers need a current runtime while executing on
// the simulation thread.
thread_local! {
    // Lets hosted glue recover the active runtime handle without passing it
    // through every call. This should stay a convenience, not the primary API.
    static CURRENT_HANDLE: RefCell<Option<sim::Handle>> = const { RefCell::new(None) };
    // Feeds deterministic bytes to host randomness calls made during an active
    // simulation. Every such request advances the runtime RNG trace.
    static CURRENT_RNG: RefCell<Option<sim::GlobalRng>> = const { RefCell::new(None) };
    // Marks the current OS thread as simulation-owned so thread creation hooks
    // can reject accidental escapes to the host scheduler.
    static IN_SIMULATION: Cell<bool> = const { Cell::new(false) };
}

struct CurrentHandleGuard {
    previous: Option<sim::Handle>,
}

struct CurrentRngGuard {
    previous: Option<sim::GlobalRng>,
}

struct SimulationThreadGuard {
    previous: bool,
}

fn enter_handle_context(handle: sim::Handle) -> CurrentHandleGuard {
    let previous = CURRENT_HANDLE.with(|slot| slot.borrow_mut().replace(handle));
    CurrentHandleGuard { previous }
}

fn enter_simulation_thread() -> SimulationThreadGuard {
    let previous = IN_SIMULATION.with(|state| state.replace(true));
    SimulationThreadGuard { previous }
}

fn enter_rng_context(rng: sim::GlobalRng) -> CurrentRngGuard {
    let previous = CURRENT_RNG.with(|current| current.replace(Some(rng)));
    CurrentRngGuard { previous }
}

fn in_simulation() -> bool {
    IN_SIMULATION.with(Cell::get)
}

impl Drop for CurrentHandleGuard {
    fn drop(&mut self) {
        CURRENT_HANDLE.with(|slot| {
            *slot.borrow_mut() = self.previous.take();
        });
    }
}

impl Drop for CurrentRngGuard {
    fn drop(&mut self) {
        CURRENT_RNG.with(|current| {
            current.replace(self.previous.take());
        });
    }
}

impl Drop for SimulationThreadGuard {
    fn drop(&mut self) {
        IN_SIMULATION.with(|state| {
            state.set(self.previous);
        });
    }
}

// Randomness hook helpers.

// Make sure our exported random hook is present in the final test binary.
//
// Some platforms only resolve getrandom/getentropy lazily. Calling it with a
// zero-length buffer is a no-op for behavior, but forces the symbol path to be
// linked before simulation code starts depending on it.
fn ensure_rng_hooks_linked() {
    unsafe {
        // Force the local getentropy symbol to be linked even if the host std
        // library does not call it during this particular test.
        getentropy(ptr::null_mut(), 0);
    }
}

// Fill bytes from the current runtime RNG when host code asks for randomness
// during an active simulation.
//
// This is the intentional deterministic substitute for OS randomness. If no
// simulation RNG is installed, the caller is outside `sim_std::block_on` and
// the libc hook should warn before delegating to the host OS.
fn fill_from_current_rng(buf: *mut u8, buflen: usize) -> bool {
    CURRENT_RNG.with(|current| {
        let Some(rng) = current.borrow().clone() else {
            return false;
        };
        if buflen == 0 {
            return true;
        }
        let buf = unsafe { core::slice::from_raw_parts_mut(buf, buflen) };
        rng.fill_bytes(buf);
        true
    })
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
// Code running inside simulation consumes bytes from the runtime RNG. Code
// outside simulation warns and falls back to host randomness so hosted test
// code continues to work.
#[unsafe(no_mangle)]
#[inline(never)]
unsafe extern "C" fn getrandom(buf: *mut u8, buflen: usize, flags: u32) -> isize {
    if fill_from_current_rng(buf, buflen) {
        // Randomness requested while a simulation is active is deterministic
        // and advances the runtime RNG trace.
        return buflen as isize;
    }

    eprintln!("warning: randomness requested outside simulation; delegating to host OS");
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

    use super::{enter_rng_context, getentropy};

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
    fn getentropy_uses_current_sim_rng() {
        let rng = sim::GlobalRng::new(20);
        let _guard = enter_rng_context(rng.clone());

        let mut actual = [0u8; 24];
        unsafe {
            assert_eq!(getentropy(actual.as_mut_ptr(), actual.len()), 0);
        }

        let expected_rng = sim::GlobalRng::new(20);
        let mut expected = [0u8; 24];
        expected_rng.fill_bytes(&mut expected);
        assert_eq!(actual, expected);
    }

    #[test]
    fn getentropy_delegates_to_host_randomness_outside_simulation() {
        let mut actual = [0u8; 24];
        unsafe {
            assert_eq!(getentropy(actual.as_mut_ptr(), actual.len()), 0);
        }
    }
}
