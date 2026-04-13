#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct DstSeed(pub u64);

impl DstSeed {
    pub fn fork(self, discriminator: u64) -> Self {
        // derive independent seed using same mixing primitive
        Self(splitmix64(self.0 ^ discriminator.wrapping_mul(GAMMA)))
    }

    pub fn rng(self) -> DstRng {
        DstRng {
            state: splitmix64(self.0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DstRng {
    state: u64,
}

impl DstRng {
    pub fn next_u64(&mut self) -> u64 {
        // advance state, then reuse splitmix64 mixing
        self.state = self.state.wrapping_add(GAMMA);
        splitmix64(self.state)
    }

    pub fn index(&mut self, len: usize) -> usize {
        assert!(len > 0, "len must be non-zero");
        (self.next_u64() as usize) % len
    }
}

// constants reused everywhere
const GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

/// Reference: https://rosettacode.org/wiki/Pseudo-random_numbers/Splitmix64
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(GAMMA);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::DstSeed;

    #[test]
    fn fork_is_stable_and_distinct() {
        let seed = DstSeed(7);
        assert_eq!(seed.fork(1), seed.fork(1));
        assert_ne!(seed.fork(1), seed.fork(2));
    }

    #[test]
    fn rng_sequence_is_replayable() {
        let mut a = DstSeed(99).rng();
        let mut b = DstSeed(99).rng();
        for _ in 0..8 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}
