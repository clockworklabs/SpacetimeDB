use crate::{
    algebraic_type::AlgebraicType,
    builtin_value::{self, BuiltinValue},
    product_value::{self, ProductValue},
    sum_value::{self, SumValue},
};
use enum_as_inner::EnumAsInner;
use std::fmt::Display;

#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum AlgebraicValue {
    Sum(SumValue),
    Product(ProductValue),
    Builtin(BuiltinValue),
}

pub struct SATNFormatter<'a> {
    ty: &'a AlgebraicType,
    val: &'a AlgebraicValue,
}

impl<'a> SATNFormatter<'a> {
    pub fn new(ty: &'a AlgebraicType, val: &'a AlgebraicValue) -> Self {
        Self { ty, val }
    }
}

impl<'a> Display for SATNFormatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.ty {
            AlgebraicType::Sum(ty) => {
                let val = self.val.as_sum().unwrap();
                write!(f, "{}", sum_value::SATNFormatter::new(ty, val))
            }
            AlgebraicType::Product(ty) => {
                let val = self.val.as_product().unwrap();
                write!(f, "{}", product_value::SATNFormatter::new(ty, val))
            }
            AlgebraicType::Builtin(ty) => {
                let val = self.val.as_builtin().unwrap();
                write!(f, "{}", builtin_value::SATNFormatter::new(ty, val))
            }
        }
    }
}

impl AlgebraicValue {
    pub fn decode(algebraic_type: &AlgebraicType, bytes: impl AsRef<[u8]>) -> Result<(Self, usize), &'static str> {
        let bytes = bytes.as_ref();
        match algebraic_type {
            AlgebraicType::Product(ty) => {
                let (val, nr) = ProductValue::decode(ty, &bytes[0..])?;
                Ok((Self::Product(val), nr))
            }
            AlgebraicType::Sum(ty) => {
                let (val, nr) = SumValue::decode(ty, &bytes[0..])?;
                Ok((Self::Sum(val), nr))
            }
            AlgebraicType::Builtin(ty) => {
                let (val, nr) = BuiltinValue::decode(ty, &bytes[0..])?;
                Ok((Self::Builtin(val), nr))
            }
        }
    }

    pub fn encode(&self, bytes: &mut Vec<u8>) {
        match self {
            Self::Product(v) => {
                v.encode(bytes);
            }
            Self::Sum(v) => {
                v.encode(bytes);
            }
            Self::Builtin(v) => {
                v.encode(bytes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AlgebraicType;
    use crate::{
        algebraic_type::SATNFormatter,
        algebraic_value::{self, AlgebraicValue},
        builtin_type::BuiltinType,
        builtin_value::BuiltinValue,
        product_type::ProductType,
        product_type_element::ProductTypeElement,
        product_value::ProductValue,
        sum_type::SumType,
        sum_value::SumValue,
    };

    #[test]
    fn unit() {
        let unit = AlgebraicValue::Product(ProductValue { elements: vec![] });
        assert_eq!(
            algebraic_value::SATNFormatter::new(&AlgebraicType::Product(ProductType::new(vec![])), &unit).to_string(),
            "()"
        );
    }

    #[test]
    fn product_value() {
        let product_type = AlgebraicType::Product(ProductType::new(vec![ProductTypeElement::new(
            AlgebraicType::Builtin(BuiltinType::I32),
            Some("foo".into()),
        )]));
        let product_value = AlgebraicValue::Product(ProductValue {
            elements: vec![AlgebraicValue::Builtin(BuiltinValue::I32(42))],
        });
        assert_eq!(
            "(foo: I32 = 42)",
            algebraic_value::SATNFormatter::new(&product_type, &product_value).to_string()
        );
    }

    #[test]
    fn option_some() {
        let never = AlgebraicType::Sum(SumType { types: vec![] });
        let unit = AlgebraicType::Product(ProductType::new(vec![]));
        let some_type = AlgebraicType::Product(ProductType::new(vec![ProductTypeElement {
            algebraic_type: never.clone(),
            name: Some("some".into()),
        }]));
        let none_type = AlgebraicType::Product(ProductType::new(vec![ProductTypeElement {
            algebraic_type: unit.clone(),
            name: Some("none".into()),
        }]));
        let option = AlgebraicType::Sum(SumType {
            types: vec![some_type, none_type],
        });
        let sum_value = AlgebraicValue::Sum(SumValue {
            tag: 1,
            value: Box::new(AlgebraicValue::Product(ProductValue {
                elements: vec![AlgebraicValue::Product(ProductValue { elements: vec![] })],
            })),
        });
        assert_eq!(
            "(none: () = ())",
            algebraic_value::SATNFormatter::new(&option, &sum_value).to_string()
        );
    }

    #[test]
    fn primitive() {
        let u8 = AlgebraicType::Builtin(BuiltinType::U8);
        assert_eq!(SATNFormatter::new(&u8).to_string(), "U8");
    }
}
