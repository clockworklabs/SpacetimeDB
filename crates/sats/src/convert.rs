use crate::algebraic_value::{F32, F64};
use crate::{AlgebraicType, AlgebraicValue, ArrayValue, MapValue, ProductType, ProductValue, SumValue};
use crate::{BuiltinType, BuiltinValue};

impl From<BuiltinType> for AlgebraicType {
    fn from(x: BuiltinType) -> Self {
        AlgebraicType::Builtin(x)
    }
}

impl From<BuiltinValue> for AlgebraicValue {
    fn from(value: BuiltinValue) -> Self {
        AlgebraicValue::Builtin(value)
    }
}

impl crate::Value for AlgebraicValue {
    type Type = AlgebraicType;
}

impl From<ProductValue> for AlgebraicValue {
    fn from(x: ProductValue) -> Self {
        Self::Product(x)
    }
}

impl From<AlgebraicValue> for ProductValue {
    fn from(x: AlgebraicValue) -> Self {
        Self { elements: [x].into() }
    }
}

impl From<&AlgebraicValue> for ProductValue {
    fn from(x: &AlgebraicValue) -> Self {
        x.clone().into()
    }
}

impl From<SumValue> for AlgebraicValue {
    fn from(x: SumValue) -> Self {
        Self::Sum(x)
    }
}

impl From<ArrayValue> for AlgebraicValue {
    fn from(x: ArrayValue) -> Self {
        Self::ArrayOf(x)
    }
}

impl From<MapValue> for AlgebraicValue {
    fn from(x: MapValue) -> Self {
        Self::map(x)
    }
}

impl From<AlgebraicType> for ProductType {
    fn from(x: AlgebraicType) -> Self {
        Self::new([x.into()].into())
    }
}

impl From<ProductType> for AlgebraicType {
    fn from(x: ProductType) -> Self {
        Self::Product(x)
    }
}

macro_rules! built_in {
    ($native:ty, $kind:ident) => {
        impl From<$native> for AlgebraicValue {
            fn from(x: $native) -> Self {
                AlgebraicValue::Builtin(BuiltinValue::$kind(x))
            }
        }
    };
}

macro_rules! built_in_into {
    ($native:ty, $kind:ident) => {
        impl From<$native> for BuiltinValue {
            fn from(x: $native) -> Self {
                BuiltinValue::$kind(x.into())
            }
        }

        impl From<$native> for AlgebraicValue {
            fn from(x: $native) -> Self {
                AlgebraicValue::Builtin(x.into())
            }
        }
    };
}

built_in!(bool, Bool);
built_in!(i8, I8);
built_in!(u8, U8);
built_in!(i16, I16);
built_in!(u16, U16);
built_in!(i32, I32);
built_in!(u32, U32);
built_in!(i64, I64);
built_in!(u64, U64);
built_in!(i128, I128);
built_in!(u128, U128);
built_in_into!(f32, F32);
built_in_into!(f64, F64);
built_in_into!(F32, F32);
built_in_into!(F64, F64);
built_in!(String, String);
built_in_into!(&str, String);
built_in_into!(&[u8], Bytes);
