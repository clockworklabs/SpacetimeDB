//! Std-hosted entry points for running the deterministic simulator in tests.
//!
//! The portable simulator lives in [`crate::sim`]. This module is deliberately
//! host-specific: it marks a thread as simulation-owned while hosted tests run,
//! checks determinism by replaying a seed in fresh OS threads, and intercepts a
//! few libc calls so std code cannot silently escape determinism.

#![allow(clippy::disallowed_macros)]

use core::{cell::RefCell, future::Future, marker::PhantomData};
use std::boxed::Box;
use std::sync::OnceLock;

use crate::{sim, Handle};

// Public entry points.

/// Run a future to completion with std-hosted determinism guards installed.
///
/// This wraps [`sim::Runtime::block_on`] and is the normal entry point for DST
/// tests that execute inside a hosted process. While the future runs, this
/// marks the thread as inside simulation so OS thread spawns can be rejected.
pub fn block_on<F: Future>(runtime: &mut sim::Runtime, future: F) -> F::Output {
    let _guard = enter(Handle::simulation(runtime.handle()));
    let _io_guard = enter_io(runtime.io());
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

// Ambient hosted state used only while sim_std is driving a simulation runtime.
//
// The simulator itself stays explicit-handle based. These thread-local slots
// let production-shaped APIs resolve to the entered simulation context while
// hosted DST code runs, and let OS hooks reject escapes from deterministic
// execution.
thread_local! {
    static SIM_RUNTIME: RefCell<Option<Handle>> = const { RefCell::new(None) };
    static SIM_IO: RefCell<Option<sim::io::SimulatorIO>> = const { RefCell::new(None) };
}

#[must_use = "simulation runtime context exits immediately unless the guard is held"]
pub struct EnterGuard {
    // `active` means this guard installed the thread-local runtime and is
    // responsible for clearing it on drop. Reentrant enters for the same
    // simulation runtime return an inactive guard so sync helper functions can
    // call `runtime.enter()` inside an already-entered DST run without ending
    // the outer context when the helper returns.
    active: bool,
    _not_send: PhantomData<std::rc::Rc<()>>,
}

struct IoGuard {
    previous: Option<sim::io::SimulatorIO>,
}

/// Enter a simulated runtime on the current OS thread.
///
/// This is hosted glue for DST and tests. It intentionally lives outside
/// runtime-core because `thread_local!` and process hooks are std-only.
pub(crate) fn enter(handle: Handle) -> EnterGuard {
    assert!(
        matches!(&handle, Handle::Simulation(_)),
        "sim_std::enter requires a simulation runtime handle"
    );
    let active = SIM_RUNTIME.with(|current| {
        let mut current = current.borrow_mut();
        if current.is_some() {
            return false;
        }

        *current = Some(handle);
        true
    });
    EnterGuard {
        active,
        _not_send: PhantomData,
    }
}

fn enter_io(io: Option<sim::io::SimulatorIO>) -> IoGuard {
    let previous = SIM_IO.with(|current| current.replace(io));
    IoGuard { previous }
}

/// Return the simulated runtime currently entered on this OS thread, if any.
pub fn try_current_handle() -> Option<Handle> {
    SIM_RUNTIME.with(|current| current.borrow().clone())
}

/// Return the simulated runtime currently entered on this OS thread.
pub fn current_handle() -> Handle {
    try_current_handle().expect("simulation runtime API used outside runtime.enter")
}

/// Return the simulator I/O handle entered by the current simulation runtime.
pub fn current_io() -> sim::io::SimulatorIO {
    try_current_io().expect("current_io called outside a simulation runtime with I/O enabled")
}

/// Return the current simulator I/O handle, if the active runtime has one.
pub fn try_current_io() -> Option<sim::io::SimulatorIO> {
    SIM_IO.with(|current| current.borrow().clone())
}

fn in_simulation() -> bool {
    try_current_handle().is_some()
}

impl Drop for EnterGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }

        SIM_RUNTIME.with(|current| {
            let old = current.borrow_mut().take();
            assert!(old.is_some(), "simulation context guard dropped without enter");
        });
    }
}

impl Drop for IoGuard {
    fn drop(&mut self) {
        SIM_IO.with(|current| {
            current.replace(self.previous.take());
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

#[cfg(target_os = "macos")]
fn real_getrandom() -> unsafe extern "C" fn(*mut u8, usize, u32) -> isize {
    unsafe extern "C" fn macos_getrandom(buf: *mut u8, buflen: usize, flags: u32) -> isize {
        if flags != 0 {
            return -1;
        }

        let mut filled = 0;
        while filled < buflen {
            let chunk_len = (buflen - filled).min(256);
            if unsafe { real_getentropy()(buf.add(filled).cast(), chunk_len) } != 0 {
                return -1;
            }
            filled += chunk_len;
        }
        buflen as isize
    }

    macos_getrandom
}

#[cfg(target_os = "macos")]
fn real_getentropy() -> unsafe extern "C" fn(*mut libc::c_void, usize) -> libc::c_int {
    type GetentropyFn = unsafe extern "C" fn(*mut libc::c_void, usize) -> libc::c_int;
    static GETENTROPY: OnceLock<GetentropyFn> = OnceLock::new();
    *GETENTROPY.get_or_init(|| unsafe {
        let ptr = libc::dlsym(libc::RTLD_NEXT, c"getentropy".as_ptr().cast());
        assert!(!ptr.is_null(), "failed to resolve original getentropy");
        std::mem::transmute(ptr)
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
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
