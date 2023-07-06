pub mod satn;

use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::ValueSerializer;
use crate::meta_type::MetaType;
use crate::{de::Deserialize, ser::Serialize};
use crate::{AlgebraicType, AlgebraicTypeRef, AlgebraicValue, ProductType, ProductTypeElement, SumTypeVariant};
use enum_as_inner::EnumAsInner;

/// Represents the built-in types in SATS.
///
/// Some of these types are nominal in our otherwise structural type system.
#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub enum BuiltinType {
    /// The bool type. Values `BuiltinValue::Bool(b)` will have this type.
    Bool,
    /// The `I8` type. Values `BuiltinValue::I8(v)` will have this type.
    I8,
    /// The `U8` type. Values `BuiltinValue::U8(v)` will have this type.
    U8,
    /// The `I16` type. Values `BuiltinValue::I16(v)` will have this type.
    I16,
    /// The `U16` type. Values `BuiltinValue::U16(v)` will have this type.
    U16,
    /// The `I32` type. Values `BuiltinValue::I32(v)` will have this type.
    I32,
    /// The `U32` type. Values `BuiltinValue::U32(v)` will have this type.
    U32,
    /// The `I64` type. Values `BuiltinValue::I64(v)` will have this type.
    I64,
    /// The `U64` type. Values `BuiltinValue::U64(v)` will have this type.
    U64,
    /// The `I128` type. Values `BuiltinValue::I128(v)` will have this type.
    I128,
    /// The `U128` type. Values `BuiltinValue::U128(v)` will have this type.
    U128,
    /// The `F32` type. Values `BuiltinValue::F32(v)` will have this type.
    F32,
    /// The `F64` type. Values `BuiltinValue::F64(v)` will have this type.
    F64,
    /// The UTF-8 encoded `String` type.
    /// Values `BuiltinValue::String(s)` will have this type.
    String, // Keep this because it is easy to just use Rust's `String` (UTF-8).
    /// The type of array values where elements are of a base type `elem_ty`.
    /// Values `BuiltinValue::Array(array)` will have this type.
    Array(ArrayType),
    /// The type of map values consisting of a key type `key_ty` and value `ty`.
    /// Values `BuiltinValue::Map(map)` will have this type.
    Map(MapType),
}

/// An array type is a homegeneous product type of dynamic length.
///
/// That is, it is a product type
/// where every element / factor / field is of the same type
/// and where the length is statically unknown.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ArrayType {
    /// The base type every element of the array has.
    pub elem_ty: Box<AlgebraicType>,
}

impl Serialize for ArrayType {
    fn serialize<S: crate::ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.elem_ty.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for ArrayType {
    fn deserialize<D: crate::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Deserialize::deserialize(deserializer).map(|elem_ty| Self { elem_ty })
    }
}

/// A map type from keys of type `key_ty` to values of type `ty`.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct MapType {
    /// The key type of the map.
    pub key_ty: Box<AlgebraicType>,
    /// The value type of the map.
    pub ty: Box<AlgebraicType>,
}

impl MapType {
    /// Returns a map type with keys of type `key` and values of type `value`.
    pub fn new(key: AlgebraicType, value: AlgebraicType) -> Self {
        Self {
            key_ty: Box::new(key),
            ty: Box::new(value),
        }
    }
}

impl MetaType for BuiltinType {
    fn meta_type() -> AlgebraicType {
        let product = |elements| AlgebraicType::Product(ProductType { elements });
        let unit = || product(Vec::new());
        let zero_ref = || AlgebraicType::Ref(AlgebraicTypeRef(0));
        // TODO: sats(rename_all = "lowercase"), otherwise json won't work.
        AlgebraicType::sum(vec![
            SumTypeVariant::new_named(unit(), "bool"),
            SumTypeVariant::new_named(unit(), "i8"),
            SumTypeVariant::new_named(unit(), "u8"),
            SumTypeVariant::new_named(unit(), "i16"),
            SumTypeVariant::new_named(unit(), "u16"),
            SumTypeVariant::new_named(unit(), "i32"),
            SumTypeVariant::new_named(unit(), "u32"),
            SumTypeVariant::new_named(unit(), "i64"),
            SumTypeVariant::new_named(unit(), "u64"),
            SumTypeVariant::new_named(unit(), "i128"),
            SumTypeVariant::new_named(unit(), "u128"),
            SumTypeVariant::new_named(unit(), "f32"),
            SumTypeVariant::new_named(unit(), "f64"),
            SumTypeVariant::new_named(unit(), "string"),
            SumTypeVariant::new_named(zero_ref(), "array"),
            SumTypeVariant::new_named(
                product(vec![
                    ProductTypeElement::new_named(zero_ref(), "key_ty"),
                    ProductTypeElement::new_named(zero_ref(), "ty"),
                ]),
                "map",
            ),
        ])
    }
}

impl BuiltinType {
    pub fn as_value(&self) -> AlgebraicValue {
        self.serialize(ValueSerializer).unwrap_or_else(|x| match x {})
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<BuiltinType, ValueDeserializeError> {
        Self::deserialize(ValueDeserializer::from_ref(value))
    }
}
