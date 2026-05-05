use std::sync::atomic::{AtomicU64, Ordering};

use crate::seed::DstSeed;

const GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: DstSeed) -> Self {
        Self {
            state: splitmix64(seed.0),
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        splitmix64(self.state)
    }

    pub fn index(&mut self, len: usize) -> usize {
        assert!(len > 0, "len must be non-zero");
        (self.next_u64() as usize) % len
    }

    pub fn sample_probability(&mut self, probability: f64) -> bool {
        probability_sample(self.next_u64(), probability)
    }
}

#[derive(Debug)]
pub(crate) struct DecisionSource {
    state: AtomicU64,
}

impl DecisionSource {
    pub(crate) fn new(seed: DstSeed) -> Self {
        Self {
            state: AtomicU64::new(splitmix64(seed.0)),
        }
    }

    pub(crate) fn sample_probability(&self, probability: f64) -> bool {
        probability_sample(self.next_u64(), probability)
    }

    fn next_u64(&self) -> u64 {
        let state = self.state.fetch_add(GAMMA, Ordering::Relaxed);
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
