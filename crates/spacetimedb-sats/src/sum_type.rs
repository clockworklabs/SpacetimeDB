pub mod satn;
use crate::{
    algebraic_type::AlgebraicType, algebraic_type_ref::AlgebraicTypeRef, algebraic_value::AlgebraicValue,
    builtin_type::BuiltinType, builtin_value::BuiltinValue, product_type::ProductType,
    product_type_element::ProductTypeElement, product_value::ProductValue, sum_type_variant::SumTypeVariant,
    sum_value::SumValue,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct SumType {
    pub variants: Vec<SumTypeVariant>,
}

impl SumType {
    pub fn new(variants: Vec<SumTypeVariant>) -> Self {
        Self { variants }
    }

    pub fn new_unnamed(types: Vec<AlgebraicType>) -> Self {
        let variants = types
            .iter()
            .map(|ty| SumTypeVariant::new(ty.clone(), None))
            .collect::<Vec<_>>();
        Self { variants }
    }
}

impl SumType {
    pub fn make_meta_type() -> AlgebraicType {
        let string = AlgebraicType::Builtin(BuiltinType::String);
        let option = AlgebraicType::make_option_type(string);
        let variant_type = AlgebraicType::Product(ProductType::new(vec![
            ProductTypeElement {
                algebraic_type: option,
                name: Some("name".into()),
            },
            ProductTypeElement {
                algebraic_type: AlgebraicType::Ref(AlgebraicTypeRef(0)),
                name: Some("algebraic_type".into()),
            },
        ]));
        let array = AlgebraicType::Builtin(BuiltinType::Array {
            ty: Box::new(variant_type),
        });
        AlgebraicType::Product(ProductType::new(vec![ProductTypeElement {
            algebraic_type: array,
            name: Some("variants".into()),
        }]))
    }

    pub fn as_value(&self) -> AlgebraicValue {
        let mut variants = Vec::new();
        for variant in &self.variants {
            let variant_value = if let Some(name) = variant.name.clone() {
                AlgebraicValue::Product(ProductValue {
                    elements: vec![
                        AlgebraicValue::Sum(SumValue {
                            tag: 0,
                            value: Box::new(AlgebraicValue::Builtin(BuiltinValue::String(name))),
                        }),
                        variant.algebraic_type.as_value(),
                    ],
                })
            } else {
                AlgebraicValue::Product(ProductValue {
                    elements: vec![
                        AlgebraicValue::Sum(SumValue {
                            tag: 1,
                            value: Box::new(AlgebraicValue::Product(ProductValue { elements: Vec::new() })),
                        }),
                        variant.algebraic_type.as_value(),
                    ],
                })
            };
            variants.push(variant_value);
        }
        AlgebraicValue::Product(ProductValue {
            elements: vec![AlgebraicValue::Builtin(BuiltinValue::Array { val: variants })],
        })
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<SumType, ()> {
        match value {
            AlgebraicValue::Sum(_) => Err(()),
            AlgebraicValue::Product(value) => {
                if value.elements.len() != 1 {
                    return Err(());
                }
                let variants = &value.elements[0];
                let Some(variants) = variants.as_builtin() else {
                    return Err(());
                };
                let Some(variants) = variants.as_array() else {
                    return Err(());
                };

                let mut v = Vec::new();
                for variant in variants {
                    let Some(variant) = variant.as_product() else {
                        return Err(())
                    };
                    if variant.elements.len() != 2 {
                        return Err(());
                    }
                    let name = &variant.elements[0];
                    let Some(name) = name.as_sum() else {
                        return Err(())
                    };
                    let name = if name.tag == 0 {
                        let Some(name) = name.value.as_builtin() else {
                            return Err(())
                        };
                        let Some(name) = name.as_string() else {
                            return Err(())
                        };
                        Some(name.clone())
                    } else if name.tag == 1 {
                        None
                    } else {
                        return Err(());
                    };
                    let algebraic_type = &variant.elements[1];
                    let algebraic_type = AlgebraicType::from_value(&algebraic_type)?;
                    v.push(SumTypeVariant { algebraic_type, name });
                }
                Ok(SumType { variants: v })
            }
            AlgebraicValue::Builtin(_) => Err(()),
        }
    }
}
