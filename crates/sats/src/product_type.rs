pub mod satn;

use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::ValueSerializer;
use crate::{de::Deserialize, ser::Serialize};
use crate::{AlgebraicType, AlgebraicTypeRef, AlgebraicValue, BuiltinType, ProductTypeElement};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[sats(crate = "crate")]
pub struct ProductType {
    pub elements: Vec<ProductTypeElement>,
}

impl ProductType {
    pub fn new(elements: Vec<ProductTypeElement>) -> Self {
        Self { elements }
    }
}

impl FromIterator<ProductTypeElement> for ProductType {
    fn from_iter<T: IntoIterator<Item = ProductTypeElement>>(iter: T) -> Self {
        Self::new(iter.into_iter().collect())
    }
}
impl<'a> FromIterator<(&'a str, AlgebraicType)> for ProductType {
    fn from_iter<T: IntoIterator<Item = (&'a str, AlgebraicType)>>(iter: T) -> Self {
        iter.into_iter()
            .map(|(name, ty)| ProductTypeElement::new_named(ty, name))
            .collect()
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
        self.serialize(ValueSerializer).unwrap_or_else(|x| match x {})
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<ProductType, ValueDeserializeError> {
        Self::deserialize(ValueDeserializer::from_ref(value))
    }
}
