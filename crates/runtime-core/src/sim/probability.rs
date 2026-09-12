use core::time::Duration;

use super::Rng;

/// Sample a deterministic duration from a bounded uniform range.
///
/// `mean` is the midpoint of the sampled range, not an exponential or normal
/// distribution parameter. When `mean > min`, the range is:
///
/// `[min, min + 2 * (mean - min)]`
///
/// This keeps the arithmetic simple and deterministic while preserving the
/// configured mean for a uniform distribution. If `mean <= min`, there is no
/// range to sample from and the function returns `min`.
pub(crate) fn sample_duration_between(rng: &Rng, min: Duration, mean: Duration) -> Duration {
    if mean <= min {
        return min;
    }

    let min_ns = min.as_nanos();
    let spread_ns = mean.as_nanos().saturating_sub(min_ns).saturating_mul(2);
    let spread_ns = spread_ns.min(u128::from(u64::MAX)) as u64;
    if spread_ns == 0 {
        return min;
    }

    min.saturating_add(Duration::from_nanos(rng.next_u64() % (spread_ns + 1)))
}
