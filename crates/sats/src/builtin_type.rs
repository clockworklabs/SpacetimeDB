pub mod satn;

use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::ValueSerializer;
use crate::{de::Deserialize, ser::Serialize};
use crate::{
    AlgebraicType, AlgebraicTypeRef, AlgebraicValue, ProductType, ProductTypeElement, SumType, SumTypeVariant,
};
use enum_as_inner::EnumAsInner;

#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub enum BuiltinType {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    I128,
    U128,
    F32,
    F64,
    String, // Keep this because it is easy to just use Rust's String (utf-8)
    Array(ArrayType),
    Map(MapType),
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ArrayType {
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

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct MapType {
    pub key_ty: Box<AlgebraicType>,
    pub ty: Box<AlgebraicType>,
}

impl MapType {
    pub fn new(key: AlgebraicType, value: AlgebraicType) -> Self {
        Self {
            key_ty: Box::new(key),
            ty: Box::new(value),
        }
    }
}

impl BuiltinType {
    pub fn make_meta_type() -> AlgebraicType {
        // TODO: sats(rename_all = "lowercase"), otherwise json won't work
        AlgebraicType::Sum(SumType::new(vec![
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "bool"),
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "i8"),
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "u8"),
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "i16"),
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "u16"),
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "i32"),
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "u32"),
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "i64"),
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "u64"),
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "i128"),
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "u128"),
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "f32"),
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "f64"),
            SumTypeVariant::new_named(AlgebraicType::Product(ProductType { elements: Vec::new() }), "string"),
            SumTypeVariant::new_named(AlgebraicType::Ref(AlgebraicTypeRef(0)), "array"),
            SumTypeVariant::new_named(
                AlgebraicType::Product(ProductType {
                    elements: vec![
                        ProductTypeElement::new_named(AlgebraicType::Ref(AlgebraicTypeRef(0)), "key_ty"),
                        ProductTypeElement::new_named(AlgebraicType::Ref(AlgebraicTypeRef(0)), "ty"),
                    ],
                }),
                "map",
            ),
        ]))
    }

    pub fn as_value(&self) -> AlgebraicValue {
        self.serialize(ValueSerializer).unwrap_or_else(|x| match x {})
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<BuiltinType, ValueDeserializeError> {
        Self::deserialize(ValueDeserializer::from_ref(value))
    }
}
