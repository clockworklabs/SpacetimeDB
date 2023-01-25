pub mod satn;

use crate::{
    algebraic_type::AlgebraicType, algebraic_type_ref::AlgebraicTypeRef, algebraic_value::AlgebraicValue,
    product_type::ProductType, product_type_element::ProductTypeElement, product_value::ProductValue,
    sum_type::SumType, sum_type_variant::SumTypeVariant, sum_value::SumValue,
};
use enum_as_inner::EnumAsInner;
use serde::{Deserialize, Serialize};

#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
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
    Array {
        ty: Box<AlgebraicType>,
    },
    Map {
        key_ty: Box<AlgebraicType>,
        ty: Box<AlgebraicType>,
    },
}

impl BuiltinType {
    pub fn make_meta_type() -> AlgebraicType {
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
        let tag = match self {
            BuiltinType::Bool => 0,
            BuiltinType::I8 => 1,
            BuiltinType::U8 => 2,
            BuiltinType::I16 => 3,
            BuiltinType::U16 => 4,
            BuiltinType::I32 => 5,
            BuiltinType::U32 => 6,
            BuiltinType::I64 => 7,
            BuiltinType::U64 => 8,
            BuiltinType::I128 => 9,
            BuiltinType::U128 => 10,
            BuiltinType::F32 => 11,
            BuiltinType::F64 => 12,
            BuiltinType::String => 13,
            BuiltinType::Array { ty } => {
                return AlgebraicValue::Sum(SumValue {
                    tag: 14,
                    value: Box::new(ty.as_value()),
                });
            }
            BuiltinType::Map { key_ty, ty } => {
                return AlgebraicValue::Sum(SumValue {
                    tag: 15,
                    value: Box::new(AlgebraicValue::Product(ProductValue {
                        elements: vec![key_ty.as_value(), ty.as_value()],
                    })),
                });
            }
        };
        AlgebraicValue::Sum(SumValue {
            tag,
            value: Box::new(AlgebraicValue::Product(ProductValue { elements: Vec::new() })),
        })
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<BuiltinType, ()> {
        match value {
            AlgebraicValue::Sum(value) => match value.tag {
                0 => Ok(BuiltinType::Bool),
                1 => Ok(BuiltinType::I8),
                2 => Ok(BuiltinType::U8),
                3 => Ok(BuiltinType::I16),
                4 => Ok(BuiltinType::U16),
                5 => Ok(BuiltinType::I32),
                6 => Ok(BuiltinType::U32),
                7 => Ok(BuiltinType::I64),
                8 => Ok(BuiltinType::U64),
                9 => Ok(BuiltinType::I128),
                10 => Ok(BuiltinType::U128),
                11 => Ok(BuiltinType::F32),
                12 => Ok(BuiltinType::F64),
                13 => Ok(BuiltinType::String),
                14 => {
                    let ty = Box::new(AlgebraicType::from_value(&value.value)?);
                    Ok(BuiltinType::Array { ty })
                }
                15 => {
                    let Some(value) = value.value.as_product() else {
                            return Err(());
                        };
                    if value.elements.len() != 2 {
                        return Err(());
                    }
                    let key_ty = Box::new(AlgebraicType::from_value(&value.elements[0])?);
                    let ty = Box::new(AlgebraicType::from_value(&value.elements[0])?);
                    Ok(BuiltinType::Map { key_ty, ty })
                }
                _ => Err(()),
            },
            AlgebraicValue::Product(_) => Err(()),
            AlgebraicValue::Builtin(_) => Err(()),
        }
    }
}
