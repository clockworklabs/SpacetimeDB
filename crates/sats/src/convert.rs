use crate::sum_value::SumTag;
use crate::{i256, u256};
use crate::{AlgebraicType, AlgebraicValue, ProductType, ProductValue};
use spacetimedb_primitives::{ColId, ConstraintId, IndexId, ScheduleId, SequenceId, TableId, ViewId};

impl crate::Value for AlgebraicValue {
    type Type = AlgebraicType;
}

impl<X: Into<Box<[AlgebraicValue]>>> From<X> for ProductValue {
    fn from(elements: X) -> Self {
        let elements = elements.into();
        ProductValue { elements }
    }
}

impl From<AlgebraicValue> for ProductValue {
    fn from(x: AlgebraicValue) -> Self {
        [x].into()
    }
}

impl From<AlgebraicType> for ProductType {
    fn from(x: AlgebraicType) -> Self {
        Self::new([x.into()].into())
    }
}

impl From<()> for AlgebraicValue {
    fn from((): ()) -> Self {
        AlgebraicValue::unit()
    }
}

macro_rules! built_in_into {
    ($native:ty, $kind:ident) => {
        impl From<$native> for AlgebraicValue {
            fn from(x: $native) -> Self {
                Self::$kind(x.into())
            }
        }
    };
}

built_in_into!(u128, U128);
built_in_into!(i128, I128);
built_in_into!(u256, U256);
built_in_into!(i256, I256);
built_in_into!(f32, F32);
built_in_into!(f64, F64);
built_in_into!(&str, String);
built_in_into!(String, String);
built_in_into!(&[u8], Bytes);
built_in_into!(Box<[u8]>, Bytes);
built_in_into!(SumTag, Sum);

macro_rules! system_id {
    ($name:ident) => {
        impl From<$name> for AlgebraicValue {
            fn from(value: $name) -> Self {
                value.0.into()
            }
        }
    };
}
system_id!(TableId);
system_id!(ViewId);
system_id!(ColId);
system_id!(SequenceId);
system_id!(IndexId);
system_id!(ConstraintId);
system_id!(ScheduleId);
