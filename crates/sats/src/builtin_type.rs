use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::value_serialize;
use crate::meta_type::MetaType;
use crate::{de::Deserialize, ser::Serialize};
use crate::{AlgebraicType, AlgebraicValue, ArrayType, MapType, SumTypeVariant};
use enum_as_inner::EnumAsInner;

/// Represents the built-in types in SATS.
///
/// Some of these types are nominal in our otherwise structural type system.
#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub enum BuiltinType {
    /// The bool type. Values [`BuiltinValue::Bool(b)`](crate::BuiltinValue::Bool) will have this type.
    Bool,
    /// The `I8` type. Values [`BuiltinValue::I8(v)`](crate::BuiltinValue::I8) will have this type.
    I8,
    /// The `U8` type. Values [`BuiltinValue::U8(v)`](crate::BuiltinValue::U8) will have this type.
    U8,
    /// The `I16` type. Values [`BuiltinValue::I16(v)`](crate::BuiltinValue::I16) will have this type.
    I16,
    /// The `U16` type. Values [`BuiltinValue::U16(v)`](crate::BuiltinValue::U16) will have this type.
    U16,
    /// The `I32` type. Values [`BuiltinValue::I32(v)`](crate::BuiltinValue::I32) will have this type.
    I32,
    /// The `U32` type. Values [`BuiltinValue::U32(v)`](crate::BuiltinValue::U32) will have this type.
    U32,
    /// The `I64` type. Values [`BuiltinValue::I64(v)`](crate::BuiltinValue::I64) will have this type.
    I64,
    /// The `U64` type. Values [`BuiltinValue::U64(v)`](crate::BuiltinValue::U64) will have this type.
    U64,
    /// The `I128` type. Values [`BuiltinValue::I128(v)`](crate::BuiltinValue::I128) will have this type.
    I128,
    /// The `U128` type. Values [`BuiltinValue::U128(v)`](crate::BuiltinValue::U128) will have this type.
    U128,
    /// The `F32` type. Values [`BuiltinValue::F32(v)`](crate::BuiltinValue::F32) will have this type.
    F32,
    /// The `F64` type. Values [`BuiltinValue::F64(v)`](crate::BuiltinValue::F64) will have this type.
    F64,
    /// The UTF-8 encoded `String` type.
    /// Values [`BuiltinValue::String(s)`](crate::BuiltinValue::String) will have this type.
    ///
    /// This type exists for convenience and because it is easy to just use Rust's `String` (UTF-8)
    /// as opposed to rolling your own equivalent byte-array based UTF-8 encoding.
    String,
    /// The type of array values where elements are of a base type `elem_ty`.
    /// Values [`BuiltinValue::Array(array)`](crate::BuiltinValue::Array) will have this type.
    Array(ArrayType),
    /// The type of map values consisting of a key type `key_ty` and value `ty`.
    /// Values [`BuiltinValue::Map(map)`](crate::BuiltinValue::Map) will have this type.
    /// The order of entries in a map value is observable.
    Map(Box<MapType>),
}

impl MetaType for BuiltinType {
    fn meta_type() -> AlgebraicType {
        // TODO: sats(rename_all = "lowercase"), otherwise json won't work.
        AlgebraicType::sum([
            SumTypeVariant::unit("bool"),
            SumTypeVariant::unit("i8"),
            SumTypeVariant::unit("u8"),
            SumTypeVariant::unit("i16"),
            SumTypeVariant::unit("u16"),
            SumTypeVariant::unit("i32"),
            SumTypeVariant::unit("u32"),
            SumTypeVariant::unit("i64"),
            SumTypeVariant::unit("u64"),
            SumTypeVariant::unit("i128"),
            SumTypeVariant::unit("u128"),
            SumTypeVariant::unit("f32"),
            SumTypeVariant::unit("f64"),
            SumTypeVariant::unit("string"),
            SumTypeVariant::new_named(ArrayType::meta_type(), "array"),
            SumTypeVariant::new_named(MapType::meta_type(), "map"),
        ])
    }
}

impl BuiltinType {
    pub fn as_value(&self) -> AlgebraicValue {
        value_serialize(self)
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<BuiltinType, ValueDeserializeError> {
        Self::deserialize(ValueDeserializer::from_ref(value))
    }
}
