use std::fmt::Debug;

use spacetimedb_lib::{TimeDuration, Timestamp};
use spacetimedb_sats::{
    algebraic_value::de::{ValueDeserializeError, ValueDeserializer},
    de::Deserialize,
    impl_st,
    ser::Serialize,
    AlgebraicType, AlgebraicValue,
};

/// When a scheduled reducer should execute,
/// either at a specific point in time,
/// or at regular intervals for repeating schedules.
///
/// Stored in reducer-scheduling tables as a column.
///
/// This is a special type.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ScheduleAt {
    /// A regular interval at which the repeated reducer is scheduled.
    /// Value is a [`TimeDuration`], which has nanosecond precision.
    Interval(TimeDuration),
    /// A specific time to which the reducer is scheduled.
    Time(Timestamp),
}
impl_st!([] ScheduleAt, ScheduleAt::get_type());

impl ScheduleAt {
    /// Converts the `ScheduleAt` to a `std::time::Duration` from now.
    ///
    /// Returns [`Duration::ZERO`] if `self` represents a time in the past.
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    pub fn to_duration_from_now(&self) -> std::time::Duration {
        use std::time::{Duration, SystemTime};
        match self {
            ScheduleAt::Time(time) => {
                let now = SystemTime::now();
                let time = SystemTime::from(*time);
                time.duration_since(now).unwrap_or(Duration::ZERO)
            }
            // TODO(correctness): Determine useful behavior on negative intervals,
            // as that's the case where `to_duration` fails.
            // Currently, we use the magnitude / absolute value,
            // which seems at least less stupid than clamping to zero.
            ScheduleAt::Interval(dur) => dur.to_duration_abs(),
        }
    }

    /// Get the special `AlgebraicType` for `ScheduleAt`.
    pub fn get_type() -> AlgebraicType {
        AlgebraicType::sum([
            ("Interval", AlgebraicType::time_duration()),
            ("Time", AlgebraicType::timestamp()),
        ])
    }
}

impl From<TimeDuration> for ScheduleAt {
    fn from(value: TimeDuration) -> Self {
        ScheduleAt::Interval(value)
    }
}

impl From<std::time::Duration> for ScheduleAt {
    fn from(value: std::time::Duration) -> Self {
        ScheduleAt::Interval(TimeDuration::from_duration(value))
    }
}

impl From<std::time::SystemTime> for ScheduleAt {
    fn from(value: std::time::SystemTime) -> Self {
        Timestamp::from(value).into()
    }
}

impl From<crate::Timestamp> for ScheduleAt {
    fn from(value: crate::Timestamp) -> Self {
        ScheduleAt::Time(value)
    }
}

impl TryFrom<AlgebraicValue> for ScheduleAt {
    type Error = ValueDeserializeError;
    fn try_from(value: AlgebraicValue) -> Result<Self, Self::Error> {
        ScheduleAt::deserialize(ValueDeserializer::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_sats::bsatn;

    #[test]
    fn test_bsatn_roundtrip() {
        let schedule_at = ScheduleAt::Interval(TimeDuration::from_micros(10000));
        let ser = bsatn::to_vec(&schedule_at).unwrap();
        let de = bsatn::from_slice(&ser).unwrap();
        assert_eq!(schedule_at, de);
    }

    #[test]
    fn schedule_at_is_special() {
        assert!(ScheduleAt::get_type().is_special());
    }
}
