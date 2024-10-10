use crate::{de::Deserialize, impl_deserialize, impl_serialize, impl_st, ser::Serialize, AlgebraicType};
use std::time::{Duration, SystemTime};

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize, Debug)]
#[sats(crate = crate)]
/// A point in time, measured in microseconds since the Unix epoch.
///
/// This type is intended for internal use. It can be converted to and from [`SystemTime`],
/// which is how SpacetimeDB users are intended to interact with points in time.
pub struct Timestamp {
    __timestamp_micros_since_unix_epoch: u64,
}

impl_st!([] Timestamp, AlgebraicType::timestamp());

impl Timestamp {
    pub const UNIX_EPOCH: Self = Self {
        __timestamp_micros_since_unix_epoch: 0,
    };
    pub fn to_micros_since_unix_epoch(self) -> u64 {
        self.__timestamp_micros_since_unix_epoch
    }
    pub fn from_micros_since_unix_epoch(micros: u64) -> Self {
        Self {
            __timestamp_micros_since_unix_epoch: micros,
        }
    }
    pub fn to_duration_since_unix_epoch(self) -> Duration {
        Duration::from_micros(self.to_micros_since_unix_epoch())
    }
    pub fn from_duration_since_unix_epoch(duration: Duration) -> Self {
        Self::from_micros_since_unix_epoch(
            duration
                .as_micros()
                .try_into()
                .expect("Duration since Unix epoch overflows u64 microseconds"),
        )
    }
    pub fn to_system_time(self) -> SystemTime {
        let duration = self.to_duration_since_unix_epoch();
        SystemTime::UNIX_EPOCH
            .checked_add(duration)
            .expect("Timestamp with u64 milliseconds since Unix epoch overflows SystemTime")
    }
    pub fn from_system_time(system_time: SystemTime) -> Self {
        let duration = system_time
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("SystemTime predates the Unix epoch");
        Self::from_duration_since_unix_epoch(duration)
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

impl_st!([] SystemTime, AlgebraicType::timestamp());
impl_serialize!([] SystemTime, (self, ser) => Timestamp::from_system_time(*self).serialize(ser));
impl_deserialize!([] SystemTime, de => Timestamp::deserialize(de).map(Timestamp::to_system_time));

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
        fn round_trip_timestamp_through_systemtime(micros in any::<u64>()) {
            let timestamp = Timestamp::from_micros_since_unix_epoch(micros);
            let system_time = SystemTime::from(timestamp);
            let timestamp_prime = Timestamp::from(system_time);
            prop_assert_eq!(timestamp_prime, timestamp);
            prop_assert_eq!(timestamp_prime.to_micros_since_unix_epoch(), micros);
        }
    }
}
