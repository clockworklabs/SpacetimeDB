use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::ValueSerializer;
use crate::meta_type::MetaType;
use crate::{de::Deserialize, ser::Serialize};
use crate::{AlgebraicType, AlgebraicValue, ProductTypeElement};

/// A structural product type  of the factors given by `elements`.
///
/// This is also known as `struct` and `tuple` in many languages,
/// but note that unlike most languages, sums in SATs are *structural* and not nominal.
/// The name "product" comes from category theory.
///
/// See also: https://ncatlab.org/nlab/show/product+type.
///
/// These structures are known as product types because the number of possible values in product
/// ```ignore
/// { N_0: T_0, N_1: T_1, ..., N_n: T_n }
/// ```
/// is:
/// ```ignore
/// Π (i ∈ 0..n). values(T_i)
/// ```
/// so for example, `values({ A: U64, B: Bool }) = values(U64) * values(Bool)`.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct ProductType {
    /// The factors of the product type.
    ///
    /// These factors can either be named or unnamed.
    /// When all the factors are unnamed, we can regard this as a plain tuple type.
    pub elements: Vec<ProductTypeElement>,
}

impl ProductType {
    /// Returns a product type with the given `elements` as its factors.
    pub const fn new(elements: Vec<ProductTypeElement>) -> Self {
        Self { elements }
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

impl MetaType for ProductType {
    fn meta_type() -> AlgebraicType {
        AlgebraicType::product(vec![ProductTypeElement::new_named(
            AlgebraicType::array(ProductTypeElement::meta_type()),
            "elements",
        )])
    }
}

impl ProductType {
    pub fn as_value(&self) -> AlgebraicValue {
        self.serialize(ValueSerializer).unwrap_or_else(|x| match x {})
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<ProductType, ValueDeserializeError> {
        Self::deserialize(ValueDeserializer::from_ref(value))
    }
}
