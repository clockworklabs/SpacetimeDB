use alloc::{format, string::String};
use alloc::{sync::Arc, vec::Vec};
use spin::Mutex;

pub type Rng = GlobalRng;

/// Shared deterministic RNG for the simulation core.
///
/// The simulator owns one runtime-wide RNG handle and uses it for scheduler
/// choices, probabilistic fault injection, and determinism checks. Hosted
/// conveniences such as thread-local current-RNG access and libc random hooks
/// live in `adapter::sim_std`, not here.
#[derive(Clone, Debug)]
pub struct GlobalRng {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug)]
struct Inner {
    /// Seed used to initialize the runtime RNG, carried for diagnostics and replay.
    seed: u64,
    /// Deterministic generator used for scheduler choices and fault injection decisions.
    rng: SplitMix64,
    /// Checkpoints recorded during the first determinism run.
    log: Option<Vec<u8>>,
    /// Expected checkpoints plus the number already consumed during replay.
    check: Option<(Vec<u8>, usize)>,
    /// Whether probabilistic fault injection is currently enabled for this runtime.
    buggify_enabled: bool,
}

const GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

/// Reference for SplitMix64 algorithm: https://rosettacode.org/wiki/Pseudo-random_numbers/Splitmix64
/// Splitmix64 is the default pseudo-random number generator algorithm.
/// It uses a fairly simple algorithm that, though it is considered
/// to be poor for cryptographic purposes, is very fast to calculate,
/// and is "good enough" for many random number needs.
/// It passes several fairly rigorous PRNG "fitness" tests that some more complex algorithms fail.
#[derive(Clone, Debug)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        mix64(self.state)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for chunk in dest.chunks_mut(core::mem::size_of::<u64>()) {
            let bytes = self.next_u64().to_ne_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }
}

fn mix64(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

impl GlobalRng {
    /// Create a new deterministic RNG for a simulation runtime.
    pub fn new(seed: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                seed,
                rng: SplitMix64::new(seed),
                log: None,
                check: None,
                buggify_enabled: false,
            })),
        }
    }

    pub fn next_u64(&self) -> u64 {
        self.with_inner(|inner| inner.rng.next_u64())
    }

    pub fn index(&self, len: usize) -> usize {
        assert!(len > 0, "len must be non-zero");
        (self.next_u64() as usize) % len
    }

    pub fn sample_probability(&self, probability: f64) -> bool {
        probability_sample(self.next_u64(), probability)
    }

    pub fn enable_buggify(&self) {
        self.inner.lock().buggify_enabled = true;
    }

    pub fn disable_buggify(&self) {
        self.inner.lock().buggify_enabled = false;
    }

    pub fn is_buggify_enabled(&self) -> bool {
        self.inner.lock().buggify_enabled
    }

    pub fn buggify(&self) -> bool {
        self.buggify_with_prob(0.25)
    }

    pub fn buggify_with_prob(&self, probability: f64) -> bool {
        self.is_buggify_enabled() && self.sample_probability(probability)
    }

    #[allow(dead_code)]
    pub(crate) fn seed(&self) -> u64 {
        self.inner.lock().seed
    }

    fn with_inner<T>(&self, f: impl FnOnce(&mut Inner) -> T) -> T {
        let mut inner = self.inner.lock();
        let output = f(&mut inner);
        if inner.log.is_some() || inner.check.is_some() {
            let checkpoint = checksum(inner.rng.clone().next_u64());
            if let Some(log) = &mut inner.log {
                log.push(checkpoint);
            }
            let seed = inner.seed;
            if let Some((expected, consumed)) = &mut inner.check {
                if expected.get(*consumed) != Some(&checkpoint) {
                    panic!("non-determinism detected for seed {} at checkpoint {consumed}", seed);
                }
                *consumed += 1;
            }
        }
        output
    }

    #[allow(dead_code)]
    pub(crate) fn fill_bytes(&self, dest: &mut [u8]) {
        self.with_inner(|inner| inner.rng.fill_bytes(dest));
    }

    #[allow(dead_code)]
    pub(crate) fn enable_determinism_log(&self) {
        let mut inner = self.inner.lock();
        inner.log = Some(Vec::new());
        inner.check = None;
    }

    #[allow(dead_code)]
    pub(crate) fn enable_determinism_check(&self, log: DeterminismLog) {
        let mut inner = self.inner.lock();
        inner.check = Some((log.0, 0));
        inner.log = None;
    }

    #[allow(dead_code)]
    pub(crate) fn take_determinism_log(&self) -> Option<DeterminismLog> {
        let mut inner = self.inner.lock();
        inner
            .log
            .take()
            .or_else(|| inner.check.take().map(|(log, _)| log))
            .map(DeterminismLog)
    }

    #[allow(dead_code)]
    pub(crate) fn finish_determinism_check(&self) -> Result<(), String> {
        let inner = self.inner.lock();
        if let Some((log, consumed)) = &inner.check {
            if *consumed != log.len() {
                return Err(format!(
                    "non-determinism detected for seed {}: consumed {consumed} of {} checkpoints",
                    inner.seed,
                    log.len()
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct DeterminismLog(Vec<u8>);

fn probability_sample(value: u64, probability: f64) -> bool {
    if probability <= 0.0 {
        return false;
    }
    if probability >= 1.0 {
        return true;
    }

    let unit = (value >> 11) as f64 * (1.0 / ((1u64 << 53) as f64));
    unit < probability
}

fn checksum(value: u64) -> u8 {
    value.to_ne_bytes().into_iter().fold(0, |acc, byte| acc ^ byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_log_check_accepts_same_sequence() {
        let first = Rng::new(10);
        first.enable_determinism_log();
        let first_values = (0..8).map(|_| first.next_u64()).collect::<Vec<_>>();
        let log = first.take_determinism_log().unwrap();

        let second = Rng::new(10);
        second.enable_determinism_check(log);
        let second_values = (0..8).map(|_| second.next_u64()).collect::<Vec<_>>();
        second.finish_determinism_check().unwrap();

        assert_eq!(first_values, second_values);
    }

    #[test]
    fn buggify_is_disabled_by_default() {
        let rng = Rng::new(20);
        for _ in 0..8 {
            assert!(!rng.buggify());
            assert!(!rng.buggify_with_prob(1.0));
        }
    }

    #[test]
    fn buggify_obeys_enable_and_disable() {
        let rng = Rng::new(21);
        rng.enable_buggify();
        assert!(rng.is_buggify_enabled());
        assert!(rng.buggify_with_prob(1.0));
        rng.disable_buggify();
        assert!(!rng.is_buggify_enabled());
        assert!(!rng.buggify_with_prob(1.0));
    }
}
