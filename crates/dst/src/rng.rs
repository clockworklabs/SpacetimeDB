use spacetimedb_runtime::sim::Rng;

#[derive(Clone, Copy)]
pub(crate) struct Choice<T> {
    weight: u64,
    value: T,
}

pub(crate) const fn choice<T>(weight: u64, value: T) -> Choice<T> {
    Choice { weight, value }
}

pub(crate) fn frequency<T: Copy>(rng: &Rng, choices: &[Choice<T>]) -> T {
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

pub(crate) fn pick_weighted(rng: &Rng, weights: &[u64]) -> usize {
    let total: u64 = weights.iter().sum();

    assert!(total > 0, "at least one weight must be non-zero");

    let mut selected = rng.next_u64() % total;

    for (idx, weight) in weights.iter().copied().enumerate() {
        if selected < weight {
            return idx;
        }

        selected -= weight;
    }

    unreachable!("selected value is always inside total weight")
}

pub(crate) fn choose_index(rng: &Rng, len: usize) -> Option<usize> {
    (len > 0).then(|| rng.index(len))
}

pub(crate) fn range_inclusive(rng: &Rng, lo: usize, hi: usize) -> usize {
    if lo >= hi {
        return lo;
    }
    lo + (rng.next_u64() as usize % (hi - lo + 1))
}
