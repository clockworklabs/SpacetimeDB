pub mod satn;
use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::ValueSerializer;
use crate::{de::Deserialize, ser::Serialize};
use crate::{
    AlgebraicType, AlgebraicTypeRef, AlgebraicValue, BuiltinType, ProductType, ProductTypeElement, SumTypeVariant,
};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[sats(crate = crate)]
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

    pub fn looks_like_option(&self) -> Option<&AlgebraicType> {
        match &*self.variants {
            [first, second]
                if first.name.as_deref() == Some("some")
                    && second.name.as_deref() == Some("none")
                    && second.algebraic_type == AlgebraicType::UNIT_TYPE =>
            {
                Some(&first.algebraic_type)
            }
            _ => None,
        }
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
        self.serialize(ValueSerializer).unwrap_or_else(|x| match x {})
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<SumType, ValueDeserializeError> {
        Self::deserialize(ValueDeserializer::from_ref(value))
    }
}
