use anyhow::Context;
use chrono::DateTime;

use crate::{de::Deserialize, impl_st, ser::Serialize, time_duration::TimeDuration, AlgebraicType, AlgebraicValue};
use std::fmt;
use std::ops::Add;
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
    pub fn parse_from_str(str: &str) -> anyhow::Result<Timestamp> {
        DateTime::parse_from_rfc3339(str)
            .map_err(|err| anyhow::anyhow!(err))
            .with_context(|| "Invalid timestamp format. Expected RFC 3339 format (e.g. '2025-02-10 15:45:30').")
            .map(|dt| dt.timestamp_micros())
            .map(Timestamp::from_micros_since_unix_epoch)
    }
}

impl Add<TimeDuration> for Timestamp {
    type Output = Self;

    fn add(self, other: TimeDuration) -> Self::Output {
        Timestamp::from_micros_since_unix_epoch(self.to_micros_since_unix_epoch() + other.to_micros())
    }
}

pub(crate) const MICROSECONDS_PER_SECOND: i64 = 1_000_000;

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let micros = self.to_micros_since_unix_epoch();
        let sign = if micros < 0 { "-" } else { "" };
        let pos = micros.abs();
        let secs = pos / MICROSECONDS_PER_SECOND;
        let micros_remaining = pos % MICROSECONDS_PER_SECOND;

        write!(f, "{sign}{secs}.{micros_remaining:06}",)
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
        fn add_duration(since_epoch in any::<i64>().prop_map(|n| n.abs()), duration in any::<i64>()) {
            prop_assume!(since_epoch.checked_add(duration).is_some());

            let timestamp = Timestamp::from_micros_since_unix_epoch(since_epoch);
            let time_duration = TimeDuration::from_micros(duration);
            let result = timestamp + time_duration;
            prop_assert_eq!(result.to_micros_since_unix_epoch(), since_epoch + duration);
        }
    }
}
