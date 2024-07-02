use std::fmt::Debug;

use spacetimedb_bindings_macro::{Deserialize, Serialize};
use spacetimedb_sats::{
    algebraic_value::de::{ValueDeserializeError, ValueDeserializer},
    de::Deserialize as _,
    impl_deserialize, impl_serialize, impl_st,
    typespace::TypespaceBuilder,
    AlgebraicType, AlgebraicValue,
};

use crate::Timestamp;

/// A span of time, in number of microseconds.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(transparent)]
pub struct Duration(pub u64);

impl Duration {
    pub fn get_type() -> AlgebraicType {
        AlgebraicType::U64
    }
}

impl From<Duration> for std::time::Duration {
    fn from(value: Duration) -> Self {
        Self::from_micros(value.0)
    }
}

impl From<std::time::Duration> for Duration {
    fn from(value: std::time::Duration) -> Self {
        Self(value.as_micros() as u64)
    }
}

impl_st!([] Duration, _ts => Duration::get_type());
impl_deserialize!([] Duration, de => u64::deserialize(de).map(Self));
impl_serialize!([] Duration, (self, ser) => self.0.serialize(ser));

/// When a scheduled reducer should execute,
/// either at a specific point in time,
/// or at regular intervals for repeating schedules.
///
/// Stored in reducer-scheduling tables as a column.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ScheduleAt {
    /// A specific time to which the reducer is scheduled.
    Time(Timestamp),
    /// A regular interval at which the repeated reducer is scheduled.
    Interval(Duration),
}

impl ScheduleAt {
    // Todo: convert it to macro, to be re-used by other types like `Address` and `Identity`
    fn make_type<S: TypespaceBuilder>(_ts: &mut S) -> AlgebraicType {
        TypespaceBuilder::add(
            _ts,
            {
                struct __Marker;
                core::any::TypeId::of::<__Marker>()
            },
            Some("ScheduleAt"),
            |__typespace| ScheduleAt::get_type(),
        )
    }
    fn get_type() -> AlgebraicType {
        AlgebraicType::sum([("Time", Timestamp::get_type()), ("Duration", Duration::get_type())])
    }

    /// Converts the `ScheduleAt` to a `std::time::Duration` from now.
    pub fn to_duration_from_now(&self) -> std::time::Duration {
        match self {
            ScheduleAt::Time(time) => time.to_duration_from_now(),
            ScheduleAt::Interval(dur) => (*dur).into(),
        }
    }
}

impl TryFrom<AlgebraicValue> for ScheduleAt {
    type Error = ValueDeserializeError;
    fn try_from(value: AlgebraicValue) -> Result<Self, Self::Error> {
        ScheduleAt::deserialize(ValueDeserializer::new(value))
    }
}

impl_st!([] ScheduleAt, _ts => ScheduleAt::make_type(_ts));

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_sats::bsatn;

    #[test]
    fn test_bsatn_roundtrip() {
        let schedule_at = ScheduleAt::Interval(Duration(10000));
        let ser = bsatn::to_vec(&schedule_at).unwrap();
        let de = bsatn::from_slice(&ser).unwrap();
        assert_eq!(schedule_at, de);
    }
}
