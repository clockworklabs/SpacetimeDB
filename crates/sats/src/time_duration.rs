use crate::timestamp::MICROSECONDS_PER_SECOND;
use crate::{de::Deserialize, impl_st, ser::Serialize, AlgebraicType};
use std::fmt;
use std::time::Duration;

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, Debug)]
#[sats(crate = crate)]
/// A span or delta in time, measured in microseconds.
///
/// Analogous to [`std::time::Duration`], and to C#'s `TimeSpan`.
/// Name chosen to avoid ambiguity with either of those types.
///
/// Unlike [`Duration`], but like C#'s `TimeSpan`,
/// `TimeDuration` can represent negative values.
/// It also offers less range than [`Duration`], so conversions in both directions may fail.
pub struct TimeDuration {
    __time_duration_micros__: i64,
}

impl_st!([] TimeDuration, AlgebraicType::time_duration());

impl TimeDuration {
    pub const ZERO: TimeDuration = TimeDuration {
        __time_duration_micros__: 0,
    };

    /// Get the number of microseconds `self` represents.
    pub fn to_micros(self) -> i64 {
        self.__time_duration_micros__
    }

    /// Construct a [`TimeDuration`] which is `micros` microseconds.
    ///
    /// A positive value means a time after the Unix epoch,
    /// and a negative value means a time before.
    pub fn from_micros(micros: i64) -> Self {
        Self {
            __time_duration_micros__: micros,
        }
    }

    /// Returns `Err(abs(self) as Duration)` if `self` is negative.
    pub fn to_duration(self) -> Result<Duration, Duration> {
        let micros = self.to_micros();
        if micros >= 0 {
            Ok(Duration::from_micros(micros as u64))
        } else {
            Err(Duration::from_micros((-micros) as u64))
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
    /// Panics if `duration.as_micros` overflows an `i64`
    pub fn from_duration(duration: Duration) -> Self {
        Self::from_micros(
            duration
                .as_micros()
                .try_into()
                .expect("Duration overflows i64 microseconds"),
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
        let micros = self.to_micros();
        let sign = if micros < 0 { "-" } else { "+" };
        let pos = micros.abs();
        let secs = pos / MICROSECONDS_PER_SECOND;
        let micros_remaining = pos % MICROSECONDS_PER_SECOND;
        write!(f, "{sign}{secs}.{micros_remaining:06}")
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
        let rounded = Duration::from_micros(now.as_micros() as _);
        let time_duration = TimeDuration::from_duration(rounded);
        let now_prime = time_duration.to_duration().unwrap();
        assert_eq!(rounded, now_prime);
    }

    proptest! {
        #[test]
        fn round_trip_time_duration_through_systemtime(micros in any::<i64>().prop_map(|n| n.abs())) {
            let time_duration = TimeDuration::from_micros(micros);
            let duration = time_duration.to_duration().unwrap();
            let time_duration_prime = TimeDuration::from_duration(duration);
            prop_assert_eq!(time_duration_prime, time_duration);
            prop_assert_eq!(time_duration_prime.to_micros(), micros);
        }
    }
}
