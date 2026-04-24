//! Small proptest-inspired strategy primitives for deterministic DST generation.
//!
//! This is intentionally minimal: we keep DST's streaming execution model and
//! use strategies only for typed, composable input generation.

use std::marker::PhantomData;

use crate::seed::DstRng;

/// Typed strategy that can sample values from the shared deterministic RNG.
pub(crate) trait Strategy<T>: Sized {
    fn sample(&self, rng: &mut DstRng) -> T;

    #[allow(dead_code)]
    fn map<U, F>(self, f: F) -> Map<Self, F, T>
    where
        F: Fn(T) -> U,
    {
        Map {
            inner: self,
            f,
            _marker: PhantomData,
        }
    }
}

/// `map` combinator for strategies.
#[allow(dead_code)]
pub(crate) struct Map<S, F, T> {
    inner: S,
    f: F,
    _marker: PhantomData<fn() -> T>,
}

impl<S, F, T, U> Strategy<U> for Map<S, F, T>
where
    S: Strategy<T>,
    F: Fn(T) -> U,
{
    fn sample(&self, rng: &mut DstRng) -> U {
        (self.f)(self.inner.sample(rng))
    }
}

/// Picks a value in `[0, upper)`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Index {
    upper: usize,
}

impl Index {
    pub(crate) fn new(upper: usize) -> Self {
        assert!(upper > 0, "index upper bound must be non-zero");
        Self { upper }
    }
}

impl Strategy<usize> for Index {
    fn sample(&self, rng: &mut DstRng) -> usize {
        rng.index(self.upper)
    }
}

/// Bernoulli-style strategy from an integer percentage in `[0, 100]`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Percent {
    percent: usize,
}

impl Percent {
    pub(crate) fn new(percent: usize) -> Self {
        Self {
            percent: percent.min(100),
        }
    }
}

impl Strategy<bool> for Percent {
    fn sample(&self, rng: &mut DstRng) -> bool {
        Index::new(100).sample(rng) < self.percent
    }
}

/// Weighted discrete choice over cloneable values.
#[derive(Clone, Debug)]
pub(crate) struct Weighted<T> {
    options: Vec<(usize, T)>,
    total_weight: usize,
}

impl<T> Weighted<T> {
    pub(crate) fn new(options: Vec<(usize, T)>) -> Self {
        let total_weight = options.iter().map(|(weight, _)| *weight).sum();
        assert!(total_weight > 0, "weighted strategy requires positive total weight");
        Self {
            options,
            total_weight,
        }
    }
}

impl<T: Clone> Strategy<T> for Weighted<T> {
    fn sample(&self, rng: &mut DstRng) -> T {
        let mut pick = Index::new(self.total_weight).sample(rng);
        for (weight, value) in &self.options {
            if pick < *weight {
                return value.clone();
            }
            pick -= *weight;
        }
        self.options
            .last()
            .map(|(_, value)| value.clone())
            .expect("weighted strategy has at least one option")
    }
}

#[cfg(test)]
mod tests {
    use crate::seed::DstSeed;

    use super::{Index, Percent, Strategy, Weighted};

    #[test]
    fn weighted_is_deterministic_for_seed() {
        let strategy = Weighted::new(vec![(1, 10usize), (2, 20usize), (3, 30usize)]);
        let mut rng_a = DstSeed(7).rng();
        let mut rng_b = DstSeed(7).rng();
        let a = (0..16).map(|_| strategy.sample(&mut rng_a)).collect::<Vec<_>>();
        let b = (0..16).map(|_| strategy.sample(&mut rng_b)).collect::<Vec<_>>();
        assert_eq!(a, b);
    }

    #[test]
    fn map_combinator_works() {
        let strategy = Percent::new(30).map(|picked| if picked { 1 } else { 0 });
        let mut rng = DstSeed(99).rng();
        let values = (0..8).map(|_| strategy.sample(&mut rng)).collect::<Vec<_>>();
        assert!(values.iter().all(|v| *v == 0 || *v == 1));
    }

    #[test]
    fn index_strategy_respects_bounds() {
        let mut rng = DstSeed(123).rng();
        for _ in 0..64 {
            let idx = Index::new(5).sample(&mut rng);
            assert!(idx < 5);
        }
    }
}
