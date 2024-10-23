use crate::timestamp::NANOSECONDS_PER_SECOND;
use crate::{de::Deserialize, impl_st, ser::Serialize, AlgebraicType};
use std::fmt;
use std::time::Duration;

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, Debug)]
#[sats(crate = crate)]
/// A span or delta in time, measured in nanoseconds.
///
/// Analogous to [`std::time::Duration`], and to C#'s `TimeSpan`.
/// Name chosen to avoid ambiguity with either of those types.
///
/// Unlike [`Duration`], but like C#'s `TimeSpan`,
/// `TimeDuration` can represent negative values.
/// It also offers less range than [`Duration`], so conversions in both directions may fail.
pub struct TimeDuration {
    __time_duration_nanos: i64,
}

impl_st!([] TimeDuration, AlgebraicType::time_duration());

impl TimeDuration {
    pub const ZERO: TimeDuration = TimeDuration {
        __time_duration_nanos: 0,
    };

    /// Get the number of nanoseconds `self` represents.
    pub fn to_nanos(self) -> i64 {
        self.__time_duration_nanos
    }

    /// Construct a [`TimeDuration`] which is `nanos` nanoseconds offset from [`Self::UNIX_EPOCH`].
    ///
    /// A positive value means a time after the Unix epoch,
    /// and a negative value means a time before.
    pub fn from_nanos(nanos: i64) -> Self {
        Self {
            __time_duration_nanos: nanos,
        }
    }

    /// Returns `Err(abs(self) as Duration)` if `self` is negative.
    pub fn to_duration(self) -> Result<Duration, Duration> {
        let nanos = self.to_nanos();
        if nanos >= 0 {
            Ok(Duration::from_nanos(nanos as u64))
        } else {
            Err(Duration::from_nanos((-nanos) as u64))
        }
    }

    /// Returns a `Duration` representing the absolute magnitude of `self`.
    ///
    /// Regardless of whether `self` is positive or negative, the returned `Duration` is positive.
    pub fn to_duration_abs(self) -> Duration {
        match self.to_duration() {
            Ok(dur) | Err(dur) => dur,
        }
    }

    /// Return a [`TimeDuration`] which represents the same span as `duration`.
    ///
    /// Panics if `duration.as_nanos` overflows an `i64`
    pub fn from_duration(duration: Duration) -> Self {
        Self::from_nanos(
            duration
                .as_nanos()
                .try_into()
                .expect("Duration since Unix epoch overflows i64 nanoseconds"),
        )
    }
}

impl From<Duration> for TimeDuration {
    fn from(d: Duration) -> TimeDuration {
        TimeDuration::from_duration(d)
    }
}

impl TryFrom<TimeDuration> for Duration {
    type Error = Duration;
    /// If `d` is negative, returns its magnitude as the `Err` variant.
    fn try_from(d: TimeDuration) -> Result<Duration, Duration> {
        d.to_duration()
    }
}

impl fmt::Display for TimeDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let nanos = self.to_nanos();
        let sign = if nanos < 0 { "-" } else { "+" };
        let pos = nanos.abs();
        let secs = pos / NANOSECONDS_PER_SECOND;
        let nanos_remaining = pos % NANOSECONDS_PER_SECOND;
        write!(f, "{sign}{secs}.{nanos_remaining:09}")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::GroundSpacetimeType;
    use proptest::prelude::*;
    use std::time::SystemTime;

    #[test]
    fn timestamp_type_matches() {
        assert_eq!(AlgebraicType::time_duration(), TimeDuration::get_type());
        assert!(TimeDuration::get_type().is_time_duration());
        assert!(TimeDuration::get_type().is_special());
    }

    #[test]
    fn round_trip_duration_through_time_duration() {
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let time_duration = TimeDuration::from_duration(now);
        let now_prime = time_duration.to_duration().unwrap();
        assert_eq!(now, now_prime);
    }

    proptest! {
        #[test]
        fn round_trip_time_duration_through_systemtime(nanos in any::<i64>().prop_map(|n| n.abs())) {
            let time_duration = TimeDuration::from_nanos(nanos);
            let duration = time_duration.to_duration().unwrap();
            let time_duration_prime = TimeDuration::from_duration(duration);
            prop_assert_eq!(time_duration_prime, time_duration);
            prop_assert_eq!(time_duration_prime.to_nanos(), nanos);
        }
    }
}
