use crate::{AlgebraicType, AlgebraicValue, ArrayType, BuiltinType, MapType, ProductType, ProductValue};
use spacetimedb_primitives::{ColId, IndexId, SequenceId, TableId};

impl crate::Value for AlgebraicValue {
    type Type = AlgebraicType;
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

impl From<AlgebraicType> for ProductType {
    fn from(x: AlgebraicType) -> Self {
        Self::new([x.into()].into())
    }
}

impl From<ArrayType> for AlgebraicType {
    fn from(x: ArrayType) -> Self {
        BuiltinType::Array(x).into()
    }
}

impl From<MapType> for AlgebraicType {
    fn from(x: MapType) -> Self {
        BuiltinType::Map(Box::new(x)).into()
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

built_in_into!(f32, F32);
built_in_into!(f64, F64);
built_in_into!(&str, String);
built_in_into!(&[u8], Bytes);

impl From<TableId> for AlgebraicValue {
    fn from(value: TableId) -> Self {
        value.0.into()
    }
}

impl From<ColId> for AlgebraicValue {
    fn from(value: ColId) -> Self {
        value.0.into()
    }
}

impl From<IndexId> for AlgebraicValue {
    fn from(value: IndexId) -> Self {
        value.0.into()
    }
}

impl From<SequenceId> for AlgebraicValue {
    fn from(value: SequenceId) -> Self {
        value.0.into()
    }
}
