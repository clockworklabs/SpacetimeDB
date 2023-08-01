pub mod satn;

use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::ValueSerializer;
use crate::{de::Deserialize, ser::Serialize};
use crate::{AlgebraicType, AlgebraicTypeRef, AlgebraicValue, ArrayType, BuiltinType, ProductTypeElement};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct ProductType {
    pub elements: Vec<ProductTypeElement>,
}

impl ProductType {
    pub fn new(elements: Vec<ProductTypeElement>) -> Self {
        Self { elements }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            elements: Vec::with_capacity(capacity),
        }
    }
}

impl From<AlgebraicType> for ProductTypeElement {
    fn from(value: AlgebraicType) -> Self {
        ProductTypeElement::new(value, None)
    }
}

impl<I: Into<ProductTypeElement>> FromIterator<I> for ProductType {
    fn from_iter<T: IntoIterator<Item = I>>(iter: T) -> Self {
        Self::new(iter.into_iter().map(Into::into).collect())
    }
}
impl<'a, I: Into<AlgebraicType>> FromIterator<(&'a str, I)> for ProductType {
    fn from_iter<T: IntoIterator<Item = (&'a str, I)>>(iter: T) -> Self {
        iter.into_iter()
            .map(|(name, ty)| ProductTypeElement::new_named(ty.into(), name))
            .collect()
    }
}

impl<'a, I: Into<AlgebraicType>> FromIterator<(Option<&'a str>, I)> for ProductType {
    fn from_iter<T: IntoIterator<Item = (Option<&'a str>, I)>>(iter: T) -> Self {
        iter.into_iter()
            .map(|(name, ty)| ProductTypeElement::new(ty.into(), name.map(str::to_string)))
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
        let array = AlgebraicType::Builtin(BuiltinType::Array(ArrayType {
            elem_ty: Box::new(element_type),
        }));
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
