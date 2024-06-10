use std::fmt::Debug;

use spacetimedb_bindings_macro::{Deserialize, Serialize};
use spacetimedb_sats::{
    algebraic_value::de::{ValueDeserializeError, ValueDeserializer},
    de::Deserialize as _,
    impl_deserialize, impl_serialize, impl_st, AlgebraicType, AlgebraicValue,
};

use crate::Timestamp;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
#[repr(transparent)]
pub struct Duration(pub u64);

impl Duration {
    pub fn get_type() -> AlgebraicType {
        AlgebraicType::U64
    }

    pub fn to_timestamp(&self, from: Timestamp) -> Timestamp {
        Timestamp(self.0 + from.0)
    }
}

impl_st!([] Duration, _ts => Duration::get_type());
impl_deserialize!([] Duration, de => u64::deserialize(de).map(Self));
impl_serialize!([] Duration, (self, ser) => self.0.serialize(ser));

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ScheduleAt {
    Time(Timestamp),
    Interval(Duration),
}

impl ScheduleAt {
    pub fn get_type() -> AlgebraicType {
        AlgebraicType::sum([Timestamp::get_type(), Duration::get_type()])
    }
}

impl TryFrom<AlgebraicValue> for ScheduleAt {
    type Error = ValueDeserializeError;
    fn try_from(value: AlgebraicValue) -> Result<Self, Self::Error> {
        ScheduleAt::deserialize(ValueDeserializer::new(value))
    }
}

impl_st!([] ScheduleAt, _ts => ScheduleAt::get_type());
