use crate::{de::Deserialize, impl_st, ser::Serialize, AlgebraicType};
use std::time::{Duration, SystemTime};

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, Debug)]
#[sats(crate = crate)]
/// A point in time, measured in nanoseconds since the Unix epoch.
pub struct Timestamp {
    __timestamp_nanos_since_unix_epoch: i64,
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
        __timestamp_nanos_since_unix_epoch: 0,
    };

    pub fn to_nanos_since_unix_epoch(self) -> i64 {
        self.__timestamp_nanos_since_unix_epoch
    }
    pub fn from_nanos_since_unix_epoch(nanos: i64) -> Self {
        Self {
            __timestamp_nanos_since_unix_epoch: nanos,
        }
    }

    /// Returns `Err(duration_before_unix_epoch)` if `self` is before `Self::UNIX_EPOCH`.
    pub fn to_duration_since_unix_epoch(self) -> Result<Duration, Duration> {
        let nanos = self.to_nanos_since_unix_epoch();
        if nanos >= 0 {
            Ok(Duration::from_nanos(nanos as u64))
        } else {
            Err(Duration::from_nanos((-nanos) as u64))
        }
    }

    /// Return a [`Timestamp`] which is [`Timestamp::UNIX_EPOCH`] plus `duration`.
    ///
    /// Panics if `duration.as_nanos` overflows an `i64`
    pub fn from_duration_since_unix_epoch(duration: Duration) -> Self {
        Self::from_nanos_since_unix_epoch(
            duration
                .as_nanos()
                .try_into()
                .expect("Duration since Unix epoch overflows i64 nanoseconds"),
        )
    }

    /// Convert `self` into a [`SystemTime`] which refers to approximately the same point in time.
    ///
    /// This conversion may lose precision, as [`SystemTime`]'s prevision varies depending on platform.
    /// E.g. Unix targets have nanosecond precision, but Windows only 100-nanosecond precision.
    ///
    /// This conversion may panic if `self` is out of bounds for [`SystemTime`].
    /// We are not aware of any platforms for which [`SystemTime`] offers a smaller range than [`Timestamp`],
    /// but such a platform may exist.
    pub fn to_system_time(self) -> SystemTime {
        match self.to_duration_since_unix_epoch() {
            Ok(positive) => SystemTime::UNIX_EPOCH
                .checked_add(positive)
                .expect("Timestamp with i64 nanoseconds since Unix epoch overflows SystemTime"),
            Err(negative) => SystemTime::UNIX_EPOCH
                .checked_sub(negative)
                .expect("Timestamp with i64 nanoseconds before Unix epoch overflows SystemTime"),
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
        let delta = self
            .to_nanos_since_unix_epoch()
            .checked_sub(earlier.to_nanos_since_unix_epoch())?;
        Self::from_nanos_since_unix_epoch(delta)
            .to_duration_since_unix_epoch()
            .ok()
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::GroundSpacetimeType;
    use proptest::prelude::*;

    #[test]
    fn timestamp_type_matches() {
        assert_eq!(AlgebraicType::timestamp(), Timestamp::get_type());
        assert!(Timestamp::get_type().is_timestamp());
        assert!(Timestamp::get_type().is_special());
    }

    #[test]
    fn round_trip_systemtime_through_timestamp() {
        let now = SystemTime::now();
        let timestamp = Timestamp::from(now);
        let now_prime = SystemTime::from(timestamp);
        assert_eq!(now, now_prime);
    }

    proptest! {
        #[test]
        fn round_trip_timestamp_through_systemtime(nanos in any::<i64>().prop_map(|n| n.abs())) {
            let timestamp = Timestamp::from_nanos_since_unix_epoch(nanos);
            let system_time = SystemTime::from(timestamp);
            let timestamp_prime = Timestamp::from(system_time);
            prop_assert_eq!(timestamp_prime, timestamp);
            prop_assert_eq!(timestamp_prime.to_nanos_since_unix_epoch(), nanos);
        }
    }
}
