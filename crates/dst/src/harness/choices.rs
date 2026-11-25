//! Recorded source of every random choice a test makes.
//!
//! All workload randomness flows through [`Choices`]. Each draw is appended to
//! a buffer, so a run can be replayed exactly from that buffer, and later
//! shrunk by editing it. Draws carry a static label, which makes failure
//! traces readable without a separate trace type per test.

use std::collections::VecDeque;
use std::ops::RangeInclusive;

/// How many recent draws a failure report shows.
const RECENT_DRAWS_MAX: usize = 32;

/// One recorded draw, kept for failure reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draw {
    pub step: u64,
    pub label: &'static str,
    /// What the draw produced: the number for `below` and `range`, 0 or 1 for
    /// `bool` and `probability`, the option index for `pick` and `weighted`.
    pub value: u64,
}

enum Source {
    /// Fresh values from a seeded generator.
    Seeded(SplitMix64),
    /// Values read back from a recorded buffer. Reads past the end yield 0,
    /// the minimal value, so an edited buffer still produces a valid run.
    Replay { values: Vec<u64>, position: usize },
}

pub struct Choices {
    source: Source,
    recorded: Vec<u64>,
    recent: VecDeque<Draw>,
    step: u64,
}

impl Choices {
    pub fn seeded(seed: u64) -> Self {
        Self::new(Source::Seeded(SplitMix64::new(seed)))
    }

    pub fn replay(values: Vec<u64>) -> Self {
        Self::new(Source::Replay { values, position: 0 })
    }

    fn new(source: Source) -> Self {
        Self {
            source,
            recorded: Vec::new(),
            recent: VecDeque::with_capacity(RECENT_DRAWS_MAX),
            step: 0,
        }
    }

    /// Every value drawn so far. Feeding this to [`Choices::replay`] reproduces the run.
    pub fn recorded(&self) -> &[u64] {
        &self.recorded
    }

    /// The most recent draws, oldest first.
    pub fn recent(&self) -> impl Iterator<Item = &Draw> {
        self.recent.iter()
    }

    pub(crate) fn set_step(&mut self, step: u64) {
        self.step = step;
    }

    /// Draw a raw `u64`. All other draws are built on this one.
    pub fn u64(&mut self, label: &'static str) -> u64 {
        let value = match &mut self.source {
            Source::Seeded(generator) => generator.next_u64(),
            Source::Replay { values, position } => {
                let value = values.get(*position).copied().unwrap_or(0);
                *position += 1;
                value
            }
        };
        self.recorded.push(value);
        if self.recent.len() == RECENT_DRAWS_MAX {
            self.recent.pop_front();
        }
        self.recent.push_back(Draw {
            step: self.step,
            label,
            value,
        });
        assert!(self.recent.len() <= RECENT_DRAWS_MAX);
        value
    }

    /// Show `result` instead of the raw value for the latest draw.
    fn note_result(&mut self, result: u64) {
        let draw = self.recent.back_mut().expect("called right after a draw");
        draw.value = result;
    }

    /// Draw a value in `0..count`.
    pub fn below(&mut self, label: &'static str, count: u64) -> u64 {
        assert!(count > 0, "Choices::below({label}) needs a non-empty range");
        let value = self.u64(label) % count;
        assert!(value < count);
        self.note_result(value);
        value
    }

    /// Draw a value in an inclusive range.
    pub fn range(&mut self, label: &'static str, range: RangeInclusive<u64>) -> u64 {
        let (low, high) = range.into_inner();
        assert!(low <= high, "Choices::range({label}) needs low <= high");
        if low == high {
            return low;
        }
        low + self.below(label, high - low + 1)
    }

    pub fn bool(&mut self, label: &'static str) -> bool {
        let result = self.u64(label).is_multiple_of(2);
        self.note_result(result as u64);
        result
    }

    /// Draw `true` with the given probability.
    pub fn probability(&mut self, label: &'static str, probability: f64) -> bool {
        let value = self.u64(label);
        let unit = (value >> 11) as f64 * (1.0 / ((1u64 << 53) as f64));
        let result = if probability <= 0.0 {
            false
        } else if probability >= 1.0 {
            true
        } else {
            unit < probability
        };
        self.note_result(result as u64);
        result
    }

    /// Pick one element of a non-empty slice.
    pub fn pick<'a, T>(&mut self, label: &'static str, items: &'a [T]) -> &'a T {
        let index = self.below(label, items.len() as u64);
        // `below` returns less than `items.len()`, which fits in `usize`.
        &items[index as usize]
    }

    /// Pick one value by weight. At least one weight must be non-zero.
    pub fn weighted<T: Copy>(&mut self, label: &'static str, options: &[(u32, T)]) -> T {
        let total: u64 = options.iter().map(|(weight, _)| *weight as u64).sum();
        let mut selected = self.below(label, total);
        for (index, (weight, value)) in options.iter().enumerate() {
            let weight = *weight as u64;
            if selected < weight {
                self.note_result(index as u64);
                return *value;
            }
            selected -= weight;
        }
        unreachable!("`selected` is below the sum of all weights")
    }
}

/// Derive an independent seed for a named purpose from a root seed.
pub fn derive_seed(root: u64, purpose: u64) -> u64 {
    mix64(root ^ mix64(purpose.wrapping_add(GAMMA)))
}

const GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

/// See <https://rosettacode.org/wiki/Pseudo-random_numbers/Splitmix64>.
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
}

fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw_some(choices: &mut Choices) -> Vec<u64> {
        vec![
            choices.u64("a"),
            choices.below("b", 7),
            choices.range("c", 3..=9),
            choices.weighted("d", &[(1, 10u64), (3, 20), (0, 30)]),
        ]
    }

    #[test]
    fn replay_reproduces_seeded_run() {
        let mut seeded = Choices::seeded(42);
        let first = draw_some(&mut seeded);

        let mut replay = Choices::replay(seeded.recorded().to_vec());
        assert_eq!(draw_some(&mut replay), first);
        assert_eq!(replay.recorded(), seeded.recorded());
    }

    #[test]
    fn replay_past_end_yields_minimal_values() {
        let mut choices = Choices::replay(vec![]);
        assert_eq!(choices.u64("a"), 0);
        assert_eq!(choices.range("b", 3..=9), 3);
        assert_eq!(choices.weighted("c", &[(0, 'x'), (2, 'y'), (1, 'z')]), 'y');
    }

    #[test]
    fn weighted_never_picks_zero_weight() {
        let mut choices = Choices::seeded(7);
        for _ in 0..1000 {
            assert_ne!(choices.weighted("w", &[(1, 'a'), (0, 'b'), (1, 'c')]), 'b');
        }
    }

    #[test]
    fn recent_keeps_last_draws_with_labels() {
        let mut choices = Choices::seeded(1);
        choices.set_step(5);
        for _ in 0..(RECENT_DRAWS_MAX + 3) {
            choices.u64("x");
        }
        assert_eq!(choices.recent().count(), RECENT_DRAWS_MAX);
        assert!(choices.recent().all(|draw| draw.label == "x" && draw.step == 5));
    }

    #[test]
    fn recent_shows_draw_results_not_raw_values() {
        let mut choices = Choices::replay(vec![17, 5, 3]);
        assert_eq!(choices.below("below", 10), 7);
        assert_eq!(choices.weighted("weighted", &[(1, 'a'), (3, 'b')]), 'b');
        assert!(!choices.bool("bool"));
        let shown: Vec<u64> = choices.recent().map(|draw| draw.value).collect();
        assert_eq!(shown, vec![7, 1, 0]);
        assert_eq!(choices.recorded(), &[17, 5, 3]);
    }

    #[test]
    fn derived_seeds_differ_by_purpose() {
        assert_ne!(derive_seed(1, 0), derive_seed(1, 1));
        assert_eq!(derive_seed(1, 0), derive_seed(1, 0));
    }
}
