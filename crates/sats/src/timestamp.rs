use anyhow::Context;
use chrono::DateTime;

use crate::{de::Deserialize, impl_st, ser::Serialize, time_duration::TimeDuration, AlgebraicType, AlgebraicValue};
use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::time::{Duration, SystemTime};

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, Debug)]
#[sats(crate = crate)]
/// A point in time, measured in microseconds since the Unix epoch.
pub struct Timestamp {
    __timestamp_micros_since_unix_epoch__: i64,
}

impl_st!([] Timestamp, AlgebraicType::timestamp());

impl Timestamp {
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    pub fn now() -> Self {
        Self::from_system_time(SystemTime::now())
    }

    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    #[deprecated = "Timestamp::now() is stubbed and will panic. Read the `.timestamp` field of a `ReducerContext` instead."]
    pub fn now() -> Self {
        unimplemented!()
    }

    pub const UNIX_EPOCH: Self = Self {
        __timestamp_micros_since_unix_epoch__: 0,
    };

    /// Get the number of microseconds `self` is offset from [`Self::UNIX_EPOCH`].
    ///
    /// A positive value means a time after the Unix epoch,
    /// and a negative value means a time before.
    pub fn to_micros_since_unix_epoch(self) -> i64 {
        self.__timestamp_micros_since_unix_epoch__
    }

    /// Construct a [`Timestamp`] which is `micros` microseconds offset from [`Self::UNIX_EPOCH`].
    ///
    /// A positive value means a time after the Unix epoch,
    /// and a negative value means a time before.
    pub fn from_micros_since_unix_epoch(micros: i64) -> Self {
        Self {
            __timestamp_micros_since_unix_epoch__: micros,
        }
    }

    pub fn from_time_duration_since_unix_epoch(time_duration: TimeDuration) -> Self {
        Self::from_micros_since_unix_epoch(time_duration.to_micros())
    }

    pub fn to_time_duration_since_unix_epoch(self) -> TimeDuration {
        TimeDuration::from_micros(self.to_micros_since_unix_epoch())
    }

    /// Returns `Err(duration_before_unix_epoch)` if `self` is before `Self::UNIX_EPOCH`.
    pub fn to_duration_since_unix_epoch(self) -> Result<Duration, Duration> {
        let micros = self.to_micros_since_unix_epoch();
        if micros >= 0 {
            Ok(Duration::from_micros(micros as u64))
        } else {
            Err(Duration::from_micros((-micros) as u64))
        }
    }

    /// Return a [`Timestamp`] which is [`Timestamp::UNIX_EPOCH`] plus `duration`.
    ///
    /// Panics if `duration.as_micros` overflows an `i64`
    pub fn from_duration_since_unix_epoch(duration: Duration) -> Self {
        Self::from_micros_since_unix_epoch(
            duration
                .as_micros()
                .try_into()
                .expect("Duration since Unix epoch overflows i64 microseconds"),
        )
    }

    /// Convert `self` into a [`SystemTime`] which refers to approximately the same point in time.
    ///
    /// This conversion may lose precision, as [`SystemTime`]'s prevision varies depending on platform.
    /// E.g. Unix targets have microsecond precision, but Windows only 100-microsecond precision.
    ///
    /// This conversion may panic if `self` is out of bounds for [`SystemTime`].
    /// We are not aware of any platforms for which [`SystemTime`] offers a smaller range than [`Timestamp`],
    /// but such a platform may exist.
    pub fn to_system_time(self) -> SystemTime {
        match self.to_duration_since_unix_epoch() {
            Ok(positive) => SystemTime::UNIX_EPOCH
                .checked_add(positive)
                .expect("Timestamp with i64 microseconds since Unix epoch overflows SystemTime"),
            Err(negative) => SystemTime::UNIX_EPOCH
                .checked_sub(negative)
                .expect("Timestamp with i64 microseconds before Unix epoch overflows SystemTime"),
        }
    }

    /// Convert a [`SystemTime`] into a [`Timestamp`] which refers to approximately the same point in time.
    ///
    /// This conversion may panic if `system_time` is out of bounds for [`Duration`].
    /// [`SystemTime`]'s range is larger than [`Timestamp`] on both Unix and Windows targets,
    /// so times in the far past or far future may panic.
    /// [`Timestamp`]'s range is approximately 292 years before and after the Unix epoch.
    pub fn from_system_time(system_time: SystemTime) -> Self {
        let duration = system_time
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("SystemTime predates the Unix epoch");
        Self::from_duration_since_unix_epoch(duration)
    }

    /// Returns the [`Duration`] delta between `self` and `earlier`, if `earlier` predates `self`.
    ///
    /// Returns `None` if `earlier` is strictly greater than `self`,
    /// or if the difference between `earlier` and `self` overflows an `i64`.
    pub fn duration_since(self, earlier: Timestamp) -> Option<Duration> {
        self.time_duration_since(earlier)?.to_duration().ok()
    }

    /// Returns the [`TimeDuration`] delta between `self` and `earlier`.
    ///
    /// The result may be negative if `earlier` is actually later than `self`.
    ///
    /// Returns `None` if the subtraction overflows or underflows `i64` microseconds.
    pub fn time_duration_since(self, earlier: Timestamp) -> Option<TimeDuration> {
        let delta = self
            .to_micros_since_unix_epoch()
            .checked_sub(earlier.to_micros_since_unix_epoch())?;
        Some(TimeDuration::from_micros(delta))
    }

    /// Parses an RFC 3339 formated timestamp string
    pub fn parse_from_rfc3339(str: &str) -> anyhow::Result<Timestamp> {
        DateTime::parse_from_rfc3339(str)
            .map_err(|err| anyhow::anyhow!(err))
            .with_context(|| "Invalid timestamp format. Expected RFC 3339 format (e.g. '2025-02-10 15:45:30').")
            .map(|dt| dt.timestamp_micros())
            .map(Timestamp::from_micros_since_unix_epoch)
    }

    /// Returns `Some(t)` where `t` is the time `self + duration` if `t` can be represented as a `Timestamp`,
    /// i.e. a 64-bit signed number of microseconds before or after the Unix epoch.
    pub fn checked_add(&self, duration: TimeDuration) -> Option<Self> {
        self.__timestamp_micros_since_unix_epoch__
            .checked_add(duration.to_micros())
            .map(Timestamp::from_micros_since_unix_epoch)
    }

    /// Returns `Some(t)` where `t` is the time `self - duration` if `t` can be represented as a `Timestamp`,
    /// i.e. a 64-bit signed number of microseconds before or after the Unix epoch.
    pub fn checked_sub(&self, duration: TimeDuration) -> Option<Self> {
        self.__timestamp_micros_since_unix_epoch__
            .checked_sub(duration.to_micros())
            .map(Timestamp::from_micros_since_unix_epoch)
    }

    /// Returns `Some(self + duration)`, or `None` if that value would be out-of-bounds for `Timestamp`.
    ///
    /// Converts `duration` into a [`TimeDuration`] before the arithmetic.
    /// Depending on the target platform's representation of [`Duration`], this may lose precision.
    pub fn checked_add_duration(&self, duration: Duration) -> Option<Self> {
        self.checked_add(TimeDuration::from_duration(duration))
    }

    /// Returns `Some(self - duration)`, or `None` if that value would be out-of-bounds for `Timestamp`.
    ///
    /// Converts `duration` into a [`TimeDuration`] before the arithmetic.
    /// Depending on the target platform's representation of [`Duration`], this may lose precision.
    pub fn checked_sub_duration(&self, duration: Duration) -> Option<Self> {
        self.checked_sub(TimeDuration::from_duration(duration))
    }
    /// Returns an RFC 3339 and ISO 8601 date and time string such as `1996-12-19T16:39:57-08:00`.
    pub fn to_rfc3339(&self) -> anyhow::Result<String> {
        DateTime::from_timestamp_micros(self.to_micros_since_unix_epoch())
            .map(|t| t.to_rfc3339())
            .ok_or_else(|| anyhow::anyhow!("Timestamp with i64 microseconds since Unix epoch overflows DateTime"))
            .with_context(|| self.to_micros_since_unix_epoch())
    }
}

impl Add<TimeDuration> for Timestamp {
    type Output = Self;

    fn add(self, other: TimeDuration) -> Self::Output {
        self.checked_add(other).unwrap()
    }
}

impl Add<Duration> for Timestamp {
    type Output = Self;

    fn add(self, other: Duration) -> Self::Output {
        self.checked_add_duration(other).unwrap()
    }
}

impl Sub<TimeDuration> for Timestamp {
    type Output = Self;

    fn sub(self, other: TimeDuration) -> Self::Output {
        self.checked_sub(other).unwrap()
    }
}

impl Sub<Duration> for Timestamp {
    type Output = Self;

    fn sub(self, other: Duration) -> Self::Output {
        self.checked_sub_duration(other).unwrap()
    }
}

impl AddAssign<TimeDuration> for Timestamp {
    fn add_assign(&mut self, other: TimeDuration) {
        *self = *self + other;
    }
}

impl AddAssign<Duration> for Timestamp {
    fn add_assign(&mut self, other: Duration) {
        *self = *self + other;
    }
}

impl SubAssign<TimeDuration> for Timestamp {
    fn sub_assign(&mut self, rhs: TimeDuration) {
        *self = *self - rhs;
    }
}

impl SubAssign<Duration> for Timestamp {
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs;
    }
}

pub(crate) const MICROSECONDS_PER_SECOND: i64 = 1_000_000;

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_rfc3339().unwrap())
    }
}

impl From<SystemTime> for Timestamp {
    fn from(system_time: SystemTime) -> Self {
        Self::from_system_time(system_time)
    }
}

impl From<Timestamp> for SystemTime {
    fn from(timestamp: Timestamp) -> Self {
        timestamp.to_system_time()
    }
}

impl From<Timestamp> for AlgebraicValue {
    fn from(value: Timestamp) -> Self {
        AlgebraicValue::product([value.to_micros_since_unix_epoch().into()])
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::GroundSpacetimeType;
    use proptest::prelude::*;

    fn round_to_micros(st: SystemTime) -> SystemTime {
        let duration = st.duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let micros = duration.as_micros();
        SystemTime::UNIX_EPOCH + Duration::from_micros(micros as _)
    }

    #[test]
    fn timestamp_type_matches() {
        assert_eq!(AlgebraicType::timestamp(), Timestamp::get_type());
        assert!(Timestamp::get_type().is_timestamp());
        assert!(Timestamp::get_type().is_special());
    }

    #[test]
    fn round_trip_systemtime_through_timestamp() {
        let now = round_to_micros(SystemTime::now());
        let timestamp = Timestamp::from(now);
        let now_prime = SystemTime::from(timestamp);
        assert_eq!(now, now_prime);
    }

    proptest! {
        #[test]
        fn round_trip_timestamp_through_systemtime(micros in any::<i64>().prop_map(|n| n.abs())) {
            let timestamp = Timestamp::from_micros_since_unix_epoch(micros);
            let system_time = SystemTime::from(timestamp);
            let timestamp_prime = Timestamp::from(system_time);
            prop_assert_eq!(timestamp_prime, timestamp);
            prop_assert_eq!(timestamp_prime.to_micros_since_unix_epoch(), micros);
        }

        #[test]
        fn arithmetic_with_timeduration(lhs in any::<i64>(), rhs in any::<i64>()) {
            let lhs_timestamp = Timestamp::from_micros_since_unix_epoch(lhs);
            let rhs_time_duration = TimeDuration::from_micros(rhs);

            if let Some(sum) = lhs.checked_add(rhs) {
                let sum_timestamp = lhs_timestamp.checked_add(rhs_time_duration);
                prop_assert!(sum_timestamp.is_some());
                prop_assert_eq!(sum_timestamp.unwrap().to_micros_since_unix_epoch(), sum);

                prop_assert_eq!((lhs_timestamp + rhs_time_duration).to_micros_since_unix_epoch(), sum);

                let mut sum_assign = lhs_timestamp;
                sum_assign += rhs_time_duration;
                prop_assert_eq!(sum_assign.to_micros_since_unix_epoch(), sum);
            } else {
                prop_assert!(lhs_timestamp.checked_add(rhs_time_duration).is_none());
            }

            if let Some(diff) = lhs.checked_sub(rhs) {
                let diff_timestamp = lhs_timestamp.checked_sub(rhs_time_duration);
                prop_assert!(diff_timestamp.is_some());
                prop_assert_eq!(diff_timestamp.unwrap().to_micros_since_unix_epoch(), diff);

                prop_assert_eq!((lhs_timestamp - rhs_time_duration).to_micros_since_unix_epoch(), diff);

                let mut diff_assign = lhs_timestamp;
                diff_assign -= rhs_time_duration;
                prop_assert_eq!(diff_assign.to_micros_since_unix_epoch(), diff);
            } else {
                prop_assert!(lhs_timestamp.checked_sub(rhs_time_duration).is_none());
            }
        }

        // TODO: determine what guarantees we provide for arithmetic with `Duration`,
        // then write tests that we uphold said guarantees.
    }
}
