use crate::algebraic_type::AlgebraicType;
use crate::algebraic_value::AlgebraicValue;
use crate::builtin_type::BuiltinType;
use crate::builtin_value::BuiltinValue;
use crate::{ProductType, ProductTypeElement, ProductValue};

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
        Self { elements: vec![x] }
    }
}

impl From<&AlgebraicValue> for ProductValue {
    fn from(x: &AlgebraicValue) -> Self {
        x.clone().into()
    }
}

impl From<AlgebraicType> for ProductType {
    fn from(x: AlgebraicType) -> Self {
        Self::new([x.into()].into())
    }
}

impl From<BuiltinType> for ProductTypeElement {
    fn from(x: BuiltinType) -> Self {
        Self::new(x.into(), None)
    }
}

macro_rules! built_in {
    ($native:ty, $kind:ident) => {
        impl From<$native> for BuiltinValue {
            fn from(x: $native) -> Self {
                BuiltinValue::$kind(x)
            }
        }

        impl From<$native> for AlgebraicValue {
            fn from(x: $native) -> Self {
                AlgebraicValue::Builtin(x.into())
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
built_in!(String, String);
built_in_into!(&str, String);
built_in_into!(&[u8], Bytes);
