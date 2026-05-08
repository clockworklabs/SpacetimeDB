use std::{
    cell::{Cell, RefCell},
    ptr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
};

const GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

#[derive(Clone, Debug)]
pub struct Rng {
    seed: u64,
    state: u64,
    log: Option<Vec<u8>>,
    check: Option<(Vec<u8>, usize)>,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        unsafe { getentropy(ptr::null_mut(), 0) };
        if !init_std_random_state(seed) {
            tracing::warn!("failed to initialize std random state, std HashMap will not be deterministic");
        }
        Self {
            seed,
            state: splitmix64(seed),
            log: None,
            check: None,
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        let value = splitmix64(self.state);
        self.record_checkpoint(value);
        value
    }

    pub fn index(&mut self, len: usize) -> usize {
        assert!(len > 0, "len must be non-zero");
        (self.next_u64() as usize) % len
    }

    pub fn sample_probability(&mut self, probability: f64) -> bool {
        probability_sample(self.next_u64(), probability)
    }

    pub(crate) fn fill_bytes(&mut self, dest: &mut [u8]) {
        for chunk in dest.chunks_mut(std::mem::size_of::<u64>()) {
            let bytes = self.next_u64().to_ne_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }

    pub(crate) fn enable_determinism_log(&mut self) {
        self.log = Some(Vec::new());
        self.check = None;
    }

    pub(crate) fn enable_determinism_check(&mut self, log: DeterminismLog) {
        self.check = Some((log.0, 0));
        self.log = None;
    }

    pub(crate) fn take_determinism_log(&mut self) -> Option<DeterminismLog> {
        self.log
            .take()
            .or_else(|| self.check.take().map(|(log, _)| log))
            .map(DeterminismLog)
    }

    pub(crate) fn finish_determinism_check(&self) -> Result<(), String> {
        if let Some((log, consumed)) = &self.check
            && *consumed != log.len()
        {
            return Err(format!(
                "non-determinism detected for seed {}: consumed {consumed} of {} checkpoints",
                self.seed,
                log.len()
            ));
        }
        Ok(())
    }

    fn record_checkpoint(&mut self, value: u64) {
        if self.log.is_none() && self.check.is_none() {
            return;
        }

        let checkpoint = checksum(value);
        if let Some(log) = &mut self.log {
            log.push(checkpoint);
        }
        if let Some((expected, consumed)) = &mut self.check {
            if expected.get(*consumed) != Some(&checkpoint) {
                panic!(
                    "non-determinism detected for seed {} at checkpoint {consumed}",
                    self.seed
                );
            }
            *consumed += 1;
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct DeterminismLog(Vec<u8>);

#[derive(Debug)]
pub struct DecisionSource {
    state: AtomicU64,
}

impl DecisionSource {
    pub fn new(seed: u64) -> Self {
        Self {
            state: AtomicU64::new(splitmix64(seed)),
        }
    }

    pub fn sample_probability(&self, probability: f64) -> bool {
        probability_sample(self.next_u64(), probability)
    }

    fn next_u64(&self) -> u64 {
        let state = self.state.fetch_add(GAMMA, Ordering::Relaxed).wrapping_add(GAMMA);
        splitmix64(state)
    }
}

fn probability_sample(value: u64, probability: f64) -> bool {
    if probability <= 0.0 {
        return false;
    }
    if probability >= 1.0 {
        return true;
    }

    // Use the top 53 bits to build an exactly representable f64 in [0, 1).
    let unit = (value >> 11) as f64 * (1.0 / ((1u64 << 53) as f64));
    unit < probability
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(GAMMA);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn checksum(value: u64) -> u8 {
    value.to_ne_bytes().into_iter().fold(0, |acc, byte| acc ^ byte)
}

thread_local! {
    static CURRENT_RNG: RefCell<Option<Arc<Mutex<Rng>>>> = const { RefCell::new(None) };
    static STD_RANDOM_SEED: Cell<Option<u64>> = const { Cell::new(None) };
}

pub(crate) struct RngContextGuard {
    previous: Option<Arc<Mutex<Rng>>>,
}

pub(crate) fn enter_rng_context(rng: Arc<Mutex<Rng>>) -> RngContextGuard {
    let previous = CURRENT_RNG.with(|current| current.replace(Some(rng)));
    RngContextGuard { previous }
}

impl Drop for RngContextGuard {
    fn drop(&mut self) {
        CURRENT_RNG.with(|current| {
            current.replace(self.previous.take());
        });
    }
}

fn init_std_random_state(seed: u64) -> bool {
    STD_RANDOM_SEED.with(|slot| slot.set(Some(seed)));
    let _ = std::collections::hash_map::RandomState::new();
    STD_RANDOM_SEED.with(|slot| slot.replace(None)).is_none()
}

fn fill_from_seed(buf: *mut u8, buflen: usize, seed: u64) {
    if buflen == 0 {
        return;
    }
    let mut state = splitmix64(seed);
    let buf = unsafe { std::slice::from_raw_parts_mut(buf, buflen) };
    for chunk in buf.chunks_mut(std::mem::size_of::<u64>()) {
        state = state.wrapping_add(GAMMA);
        let bytes = splitmix64(state).to_ne_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
}

fn fill_from_current_rng(buf: *mut u8, buflen: usize) -> bool {
    CURRENT_RNG.with(|current| {
        let Some(rng) = current.borrow().clone() else {
            return false;
        };
        if buflen == 0 {
            return true;
        }
        let buf = unsafe { std::slice::from_raw_parts_mut(buf, buflen) };
        rng.lock().expect("sim rng poisoned").fill_bytes(buf);
        true
    })
}

/// Obtain random bytes through the simulation RNG when running inside the DST executor.
///
/// This mirrors madsim's libc-level hook. It covers libc users and macOS
/// `CCRandomGenerateBytes`; crates that issue raw kernel syscalls can still
/// bypass it.
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

/// Fill a buffer with random bytes through the same hook used by libc.
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

/// macOS uses CommonCrypto for process randomness in newer Rust toolchains.
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
    use std::{collections::HashMap, sync::Arc};

    use super::*;

    #[test]
    fn rng_log_check_accepts_same_sequence() {
        let mut first = Rng::new(10);
        first.enable_determinism_log();
        let first_values = (0..8).map(|_| first.next_u64()).collect::<Vec<_>>();
        let log = first.take_determinism_log().unwrap();

        let mut second = Rng::new(10);
        second.enable_determinism_check(log);
        let second_values = (0..8).map(|_| second.next_u64()).collect::<Vec<_>>();
        second.finish_determinism_check().unwrap();

        assert_eq!(first_values, second_values);
    }

    #[test]
    fn decision_source_matches_rng_sequence() {
        let source = DecisionSource::new(12);
        let mut rng = Rng::new(12);

        for _ in 0..16 {
            assert_eq!(source.next_u64(), rng.next_u64());
        }
    }

    #[test]
    #[should_panic(expected = "non-determinism detected")]
    fn rng_log_check_rejects_different_sequence() {
        let mut first = Rng::new(10);
        first.enable_determinism_log();
        first.next_u64();
        let log = first.take_determinism_log().unwrap();

        let mut second = Rng::new(11);
        second.enable_determinism_check(log);
        second.next_u64();
    }

    #[test]
    fn getentropy_uses_current_sim_rng() {
        let rng = Arc::new(Mutex::new(Rng::new(20)));
        let _guard = enter_rng_context(Arc::clone(&rng));

        let mut actual = [0u8; 24];
        unsafe {
            assert_eq!(getentropy(actual.as_mut_ptr(), actual.len()), 0);
        }

        let mut expected_rng = Rng::new(20);
        let mut expected = [0u8; 24];
        expected_rng.fill_bytes(&mut expected);
        assert_eq!(actual, expected);
    }

    #[test]
    fn std_hashmap_order_is_seeded_for_runtime_thread() {
        fn order_for(seed: u64) -> Vec<(u64, u64)> {
            std::thread::spawn(move || {
                let _rng = Rng::new(seed);
                (0..12)
                    .map(|idx| (idx, idx))
                    .collect::<HashMap<_, _>>()
                    .into_iter()
                    .collect()
            })
            .join()
            .unwrap()
        }

        assert_eq!(order_for(30), order_for(30));
    }
}
