pub mod satn;

use crate::{
    algebraic_type::AlgebraicType, algebraic_type_ref::AlgebraicTypeRef, algebraic_value::AlgebraicValue,
    builtin_type::BuiltinType, builtin_value::BuiltinValue, product_type_element::ProductTypeElement,
    product_value::ProductValue, sum_value::SumValue,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ProductType {
    pub elements: Vec<ProductTypeElement>,
}

impl ProductType {
    pub fn new(elements: Vec<ProductTypeElement>) -> Self {
        Self { elements }
    }
}

impl ProductType {
    pub fn make_meta_type() -> AlgebraicType {
        let string = AlgebraicType::Builtin(BuiltinType::String);
        let option = AlgebraicType::make_option_type(string);
        let element_type = AlgebraicType::Product(ProductType::new(vec![
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
            ty: Box::new(element_type),
        });
        AlgebraicType::Product(ProductType::new(vec![ProductTypeElement {
            algebraic_type: array,
            name: Some("elements".into()),
        }]))
    }

    pub fn as_value(&self) -> AlgebraicValue {
        let mut elements = Vec::new();
        for element in &self.elements {
            let element_value = if let Some(name) = element.name.clone() {
                AlgebraicValue::Product(ProductValue {
                    elements: vec![
                        AlgebraicValue::Sum(SumValue {
                            tag: 0,
                            value: Box::new(AlgebraicValue::Builtin(BuiltinValue::String(name))),
                        }),
                        element.algebraic_type.as_value(),
                    ],
                })
            } else {
                AlgebraicValue::Product(ProductValue {
                    elements: vec![
                        AlgebraicValue::Sum(SumValue {
                            tag: 1,
                            value: Box::new(AlgebraicValue::Product(ProductValue { elements: Vec::new() })),
                        }),
                        element.algebraic_type.as_value(),
                    ],
                })
            };
            elements.push(element_value)
        }
        AlgebraicValue::Product(ProductValue {
            elements: vec![AlgebraicValue::Builtin(BuiltinValue::Array { val: elements })],
        })
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<ProductType, ()> {
        match value {
            AlgebraicValue::Sum(_) => Err(()),
            AlgebraicValue::Product(value) => {
                if value.elements.len() != 1 {
                    return Err(());
                }
                let elements = &value.elements[0];
                let Some(elements) = elements.as_builtin() else {
                    return Err(());
                };
                let Some(elements) = elements.as_array() else {
                    return Err(());
                };

                let mut e = Vec::new();
                for element in elements {
                    let Some(variant) = element.as_product() else {
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
                    e.push(ProductTypeElement { algebraic_type, name });
                }
                Ok(ProductType { elements: e })
            }
            AlgebraicValue::Builtin(_) => Err(()),
        }
    }
}
