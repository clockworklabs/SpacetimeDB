//! Deterministic random-selection helpers shared by DST generators.

use spacetimedb_runtime::sim::Rng;

/// A weighted value in a deterministic choice table.
#[derive(Clone, Copy)]
pub(crate) struct Choice<T> {
    weight: u64,
    value: T,
}

impl<T: Copy> Choice<T> {
    pub(crate) const fn value(self) -> T {
        self.value
    }
}

/// Construct a weighted choice entry.
pub(crate) const fn choice<T>(weight: u64, value: T) -> Choice<T> {
    Choice { weight, value }
}

/// Pick one value from `choices`, using each entry's relative weight.
pub(crate) fn pick_choice<T: Copy>(rng: &Rng, choices: &[Choice<T>]) -> T {
    let total: u64 = choices.iter().map(|choice| choice.weight).sum();

    assert!(total > 0, "at least one choice weight must be non-zero");

    let mut selected = rng.next_u64() % total;

    for choice in choices.iter().copied() {
        if selected < choice.weight {
            return choice.value;
        }

        selected -= choice.weight;
    }

    unreachable!("selected value is always inside total weight")
}

/// Weighted choice helper for enum-like generator cases.
pub(crate) trait WeightedChoice: Copy {
    fn pick(rng: &Rng, choices: &[Choice<Self>]) -> Self {
        pick_choice(rng, choices)
    }
}

/// Pick an index from `0..len`, or return `None` for an empty collection.
pub(crate) fn choose_index(rng: &Rng, len: usize) -> Option<usize> {
    (len > 0).then(|| rng.index(len))
}

/// Pick a value in the inclusive range `lo..=hi`.
pub(crate) fn range_inclusive(rng: &Rng, lo: usize, hi: usize) -> usize {
    if lo >= hi {
        return lo;
    }
    lo + (rng.next_u64() as usize % (hi - lo + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Case {
        Default,
        Mutated,
    }

    impl Case {
        const CHOICES: [Choice<Self>; 2] = [choice(1, Self::Default), choice(0, Self::Mutated)];
    }

    impl WeightedChoice for Case {}

    #[test]
    fn fixed_choice_arrays_can_be_locally_mutated() {
        let mut choices = Case::CHOICES;
        choices[0] = choice(0, Case::Default);
        choices[1] = choice(1, Case::Mutated);

        assert_eq!(Case::pick(&Rng::new(0), &choices), Case::Mutated);
        assert_eq!(Case::pick(&Rng::new(0), &Case::CHOICES), Case::Default);
    }
}
