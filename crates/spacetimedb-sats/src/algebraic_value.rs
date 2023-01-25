pub mod encoding;
pub mod satn;
use crate::{builtin_value::BuiltinValue, product_value::ProductValue, sum_value::SumValue};
use enum_as_inner::EnumAsInner;

#[derive(EnumAsInner, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum AlgebraicValue {
    Sum(SumValue),
    Product(ProductValue),
    Builtin(BuiltinValue),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::algebraic_type::AlgebraicType;
    use crate::{
        algebraic_value::{self, AlgebraicValue},
        builtin_type::BuiltinType,
        builtin_value::BuiltinValue,
        product_type::ProductType,
        product_type_element::ProductTypeElement,
        product_value::ProductValue,
        sum_value::SumValue,
        typespace::Typespace,
    };

    #[test]
    fn unit() {
        let val = AlgebraicValue::Product(ProductValue { elements: vec![] });
        let unit = AlgebraicType::Product(ProductType::new(vec![]));
        let typespace = Typespace::new(vec![]);
        assert_eq!(
            algebraic_value::satn::Formatter::new(&typespace, &unit, &val).to_string(),
            "()"
        );
    }

    #[test]
    fn product_value() {
        let product_type = AlgebraicType::Product(ProductType::new(vec![ProductTypeElement::new(
            AlgebraicType::Builtin(BuiltinType::I32),
            Some("foo".into()),
        )]));
        let typespace = Typespace::new(vec![]);
        let product_value = AlgebraicValue::Product(ProductValue {
            elements: vec![AlgebraicValue::Builtin(BuiltinValue::I32(42))],
        });
        assert_eq!(
            "(foo = 42)",
            algebraic_value::satn::Formatter::new(&typespace, &product_type, &product_value).to_string()
        );
    }

    #[test]
    fn option_some() {
        let never = AlgebraicType::make_never_type();
        let option = AlgebraicType::make_option_type(never);
        let sum_value = AlgebraicValue::Sum(SumValue {
            tag: 1,
            value: Box::new(AlgebraicValue::Product(ProductValue { elements: Vec::new() })),
        });
        let typespace = Typespace::new(vec![]);
        assert_eq!(
            "(none = ())",
            algebraic_value::satn::Formatter::new(&typespace, &option, &sum_value).to_string()
        );
    }

    #[test]
    fn primitive() {
        let u8 = AlgebraicType::Builtin(BuiltinType::U8);
        let value = AlgebraicValue::Builtin(BuiltinValue::U8(255));
        let typespace = Typespace::new(vec![]);
        assert_eq!(
            algebraic_value::satn::Formatter::new(&typespace, &u8, &value).to_string(),
            "255"
        );
    }

    #[test]
    fn array() {
        let array = AlgebraicType::Builtin(BuiltinType::Array {
            ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
        });
        let value = AlgebraicValue::Builtin(BuiltinValue::Array { val: Vec::new() });
        let typespace = Typespace::new(vec![]);
        assert_eq!(
            algebraic_value::satn::Formatter::new(&typespace, &array, &value).to_string(),
            "[]"
        );
    }

    #[test]
    fn array_of_values() {
        let array = AlgebraicType::Builtin(BuiltinType::Array {
            ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
        });
        let value = AlgebraicValue::Builtin(BuiltinValue::Array {
            val: vec![AlgebraicValue::Builtin(BuiltinValue::U8(3))],
        });
        let typespace = Typespace::new(vec![]);
        assert_eq!(
            algebraic_value::satn::Formatter::new(&typespace, &array, &value).to_string(),
            "[3]"
        );
    }

    #[test]
    fn map() {
        let map = AlgebraicType::Builtin(BuiltinType::Map {
            key_ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
            ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
        });
        let value = AlgebraicValue::Builtin(BuiltinValue::Map { val: BTreeMap::new() });
        let typespace = Typespace::new(vec![]);
        assert_eq!(
            algebraic_value::satn::Formatter::new(&typespace, &map, &value).to_string(),
            "[:]"
        );
    }

    #[test]
    fn map_of_values() {
        let map = AlgebraicType::Builtin(BuiltinType::Map {
            key_ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
            ty: Box::new(AlgebraicType::Builtin(BuiltinType::U8)),
        });
        let mut value = BTreeMap::<AlgebraicValue, AlgebraicValue>::new();
        value.insert(
            AlgebraicValue::Builtin(BuiltinValue::U8(2)),
            AlgebraicValue::Builtin(BuiltinValue::U8(3)),
        );
        let value = AlgebraicValue::Builtin(BuiltinValue::Map { val: value });
        let typespace = Typespace::new(vec![]);
        assert_eq!(
            algebraic_value::satn::Formatter::new(&typespace, &map, &value).to_string(),
            "[2: 3]"
        );
    }
}
