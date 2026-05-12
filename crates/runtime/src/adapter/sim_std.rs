use alloc::boxed::Box;
use core::{
    cell::{Cell, RefCell},
    future::Future,
    ptr,
    time::Duration,
};
use std::sync::OnceLock;

use crate::sim;

thread_local! {
    static CURRENT_HANDLE: RefCell<Option<sim::Handle>> = const { RefCell::new(None) };
    static CURRENT_RNG: RefCell<Option<sim::GlobalRng>> = const { RefCell::new(None) };
    static STD_RANDOM_SEED: Cell<Option<u64>> = const { Cell::new(None) };
    static IN_SIMULATION: Cell<bool> = const { Cell::new(false) };
}

pub(crate) struct HandleContextGuard {
    previous: Option<sim::Handle>,
}

pub(crate) struct RngContextGuard {
    previous: Option<sim::GlobalRng>,
}

pub(crate) struct SimulationThreadGuard {
    previous: bool,
}

pub fn simulation_current() -> crate::Runtime {
    crate::Runtime::simulation(current_handle().expect("simulation runtime is not active on this thread"))
}

pub fn block_on<F: Future>(runtime: &mut sim::Runtime, future: F) -> F::Output {
    ensure_rng_hooks_linked();
    if !init_std_random_state(runtime.rng().seed()) {
        tracing::warn!("failed to initialize std random state, std HashMap will not be deterministic");
    }
    let _handle_context = enter_handle_context(runtime.handle());
    let _system_thread_context = enter_simulation_thread();
    let _rng_context = enter_rng_context(runtime.rng());
    runtime.block_on(future)
}

pub fn current_handle() -> Option<sim::Handle> {
    CURRENT_HANDLE.with(|handle| handle.borrow().clone())
}

pub fn advance_time(duration: Duration) {
    current_handle()
        .expect("simulation runtime is not active on this thread")
        .advance(duration);
}

pub fn now() -> Duration {
    current_handle().map(|handle| handle.now()).unwrap_or_default()
}

pub fn sleep(duration: Duration) -> sim::time::Sleep {
    current_handle()
        .expect("sim::time::sleep polled outside sim runtime")
        .sleep(duration)
}

pub async fn timeout<T>(duration: Duration, future: impl Future<Output = T>) -> Result<T, sim::time::TimeoutElapsed> {
    current_handle()
        .expect("sim::time::timeout polled outside sim runtime")
        .timeout(duration, future)
        .await
}

pub fn check_determinism<F>(seed: u64, make_future: fn() -> F) -> F::Output
where
    F: Future + 'static,
    F::Output: Send + 'static,
{
    check_determinism_with(seed, make_future)
}

pub fn check_determinism_with<M, F>(seed: u64, make_future: M) -> F::Output
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

pub fn enable_buggify() {
    current_handle()
        .expect("simulation runtime is not active on this thread")
        .enable_buggify();
}

pub fn disable_buggify() {
    current_handle()
        .expect("simulation runtime is not active on this thread")
        .disable_buggify();
}

pub fn is_buggify_enabled() -> bool {
    current_handle().is_some_and(|handle| handle.is_buggify_enabled())
}

pub fn buggify() -> bool {
    current_handle()
        .expect("simulation runtime is not active on this thread")
        .buggify()
}

pub fn buggify_with_prob(probability: f64) -> bool {
    current_handle()
        .expect("simulation runtime is not active on this thread")
        .buggify_with_prob(probability)
}

pub(crate) fn enter_handle_context(handle: sim::Handle) -> HandleContextGuard {
    let previous = CURRENT_HANDLE.with(|slot| slot.borrow_mut().replace(handle));
    HandleContextGuard { previous }
}

pub(crate) fn enter_simulation_thread() -> SimulationThreadGuard {
    let previous = IN_SIMULATION.with(|state| state.replace(true));
    SimulationThreadGuard { previous }
}

pub(crate) fn enter_rng_context(rng: sim::GlobalRng) -> RngContextGuard {
    let previous = CURRENT_RNG.with(|current| current.replace(Some(rng)));
    RngContextGuard { previous }
}

fn in_simulation() -> bool {
    IN_SIMULATION.with(Cell::get)
}

fn init_std_random_state(seed: u64) -> bool {
    STD_RANDOM_SEED.with(|slot| slot.set(Some(seed)));
    let _ = std::collections::hash_map::RandomState::new();
    STD_RANDOM_SEED.with(|slot| slot.replace(None)).is_none()
}

fn ensure_rng_hooks_linked() {
    unsafe {
        getentropy(ptr::null_mut(), 0);
    }
}

fn fill_from_seed(buf: *mut u8, buflen: usize, seed: u64) {
    if buflen == 0 {
        return;
    }
    let rng = sim::GlobalRng::new(seed);
    let buf = unsafe { core::slice::from_raw_parts_mut(buf, buflen) };
    rng.fill_bytes(buf);
}

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

fn panic_with_seed(seed: u64, payload: Box<dyn core::any::Any + Send>) -> ! {
    eprintln!("note: run with --seed {seed} to reproduce this error");
    std::panic::resume_unwind(payload);
}

impl Drop for HandleContextGuard {
    fn drop(&mut self) {
        CURRENT_HANDLE.with(|slot| {
            *slot.borrow_mut() = self.previous.take();
        });
    }
}

impl Drop for RngContextGuard {
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

#[cfg(unix)]
#[unsafe(no_mangle)]
#[inline(never)]
unsafe extern "C" fn pthread_attr_init(attr: *mut libc::pthread_attr_t) -> libc::c_int {
    if in_simulation() {
        eprintln!("attempt to spawn a system thread in simulation.");
        eprintln!("note: use simulator tasks instead.");
        return -1;
    }

    type PthreadAttrInit = unsafe extern "C" fn(*mut libc::pthread_attr_t) -> libc::c_int;
    static PTHREAD_ATTR_INIT: OnceLock<PthreadAttrInit> = OnceLock::new();
    let original = PTHREAD_ATTR_INIT.get_or_init(|| unsafe {
        let ptr = libc::dlsym(libc::RTLD_NEXT, c"pthread_attr_init".as_ptr().cast());
        assert!(!ptr.is_null(), "failed to resolve original pthread_attr_init");
        std::mem::transmute(ptr)
    });
    unsafe { original(attr) }
}

#[unsafe(no_mangle)]
#[inline(never)]
unsafe extern "C" fn getrandom(buf: *mut u8, buflen: usize, flags: u32) -> isize {
    #[cfg(target_os = "macos")]
    let _ = flags;

    if let Some(seed) = STD_RANDOM_SEED.with(|slot| slot.replace(None)) {
        fill_from_seed(buf, buflen, seed);
        return buflen as isize;
    }
    if fill_from_current_rng(buf, buflen) {
        return buflen as isize;
    }

    #[cfg(target_os = "linux")]
    {
        type GetrandomFn = unsafe extern "C" fn(*mut u8, usize, u32) -> isize;
        static GETRANDOM: OnceLock<GetrandomFn> = OnceLock::new();
        let original = GETRANDOM.get_or_init(|| unsafe {
            let ptr = libc::dlsym(libc::RTLD_NEXT, c"getrandom".as_ptr().cast());
            assert!(!ptr.is_null(), "failed to resolve original getrandom");
            std::mem::transmute(ptr)
        });
        unsafe { original(buf, buflen, flags) }
    }

    #[cfg(target_os = "macos")]
    {
        type GetentropyFn = unsafe extern "C" fn(*mut u8, usize) -> libc::c_int;
        static GETENTROPY: OnceLock<GetentropyFn> = OnceLock::new();
        let original = GETENTROPY.get_or_init(|| unsafe {
            let ptr = libc::dlsym(libc::RTLD_NEXT, c"getentropy".as_ptr().cast());
            assert!(!ptr.is_null(), "failed to resolve original getentropy");
            std::mem::transmute(ptr)
        });
        match unsafe { original(buf, buflen) } {
            -1 => -1,
            0 => buflen as isize,
            _ => unreachable!("unexpected getentropy return value"),
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (buf, buflen, flags);
        compile_error!("unsupported OS for DST getrandom override");
    }
}

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

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
#[inline(never)]
unsafe extern "C" fn CCRandomGenerateBytes(bytes: *mut u8, count: usize) -> i32 {
    match unsafe { getrandom(bytes, count, 0) } {
        -1 => -1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use crate::sim;

    #[test]
    #[cfg(unix)]
    fn runtime_forbids_system_thread_spawn() {
        let mut runtime = sim::Runtime::new(200);
        runtime.block_on(async {
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
    fn std_hashmap_order_is_seeded_for_runtime_thread() {
        fn order_for(seed: u64) -> Vec<(u64, u64)> {
            std::thread::spawn(move || {
                let _ = init_std_random_state(seed);
                (0..12)
                    .map(|idx| (idx, idx))
                    .collect::<std::collections::HashMap<_, _>>()
                    .into_iter()
                    .collect()
            })
            .join()
            .unwrap()
        }

        assert_eq!(order_for(30), order_for(30));
    }
}
