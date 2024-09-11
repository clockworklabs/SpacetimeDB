use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::value_serialize;
use crate::de::Deserialize;
use crate::meta_type::MetaType;
use crate::{AlgebraicType, AlgebraicValue, ProductTypeElement, SpacetimeType, ValueWithType, WithTypespace};

/// The tag used inside the special `Identity` product type.
pub const IDENTITY_TAG: &str = "__identity_bytes";
/// The tag used inside the special `Address` product type.
pub const ADDRESS_TAG: &str = "__address_bytes";

/// A structural product type  of the factors given by `elements`.
///
/// This is also known as `struct` and `tuple` in many languages,
/// but note that unlike most languages, products in SATs are *[structural]* and not nominal.
/// When checking whether two nominal types are the same,
/// their names and/or declaration sites (e.g., module / namespace) are considered.
/// Meanwhile, a structural type system would only check the structure of the type itself,
/// e.g., the names of its fields and their types in the case of a record.
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
///
/// [structural]: https://en.wikipedia.org/wiki/Structural_type_system
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, SpacetimeType)]
#[sats(crate = crate)]
pub struct ProductType {
    /// The factors of the product type.
    ///
    /// These factors can either be named or unnamed.
    /// When all the factors are unnamed, we can regard this as a plain tuple type.
    pub elements: Box<[ProductTypeElement]>,
}

impl ProductType {
    /// Returns a product type with the given `elements` as its factors.
    pub const fn new(elements: Box<[ProductTypeElement]>) -> Self {
        Self { elements }
    }

    /// Returns the unit product type.
    pub fn unit() -> Self {
        Self { elements: Box::new([]) }
    }

    /// Returns whether this is a "newtype" over bytes.
    /// Does not follow `Ref`s.
    fn is_bytes_newtype(&self, check: &str) -> bool {
        match &*self.elements {
            [ProductTypeElement {
                name: Some(name),
                algebraic_type,
            }] => &**name == check && algebraic_type.is_bytes(),
            _ => false,
        }
    }

    /// Returns whether this is the special case of `spacetimedb_lib::Identity`.
    /// Does not follow `Ref`s.
    pub fn is_identity(&self) -> bool {
        self.is_bytes_newtype(IDENTITY_TAG)
    }

    /// Returns whether this is the special case of `spacetimedb_lib::Address`.
    /// Does not follow `Ref`s.
    pub fn is_address(&self) -> bool {
        self.is_bytes_newtype(ADDRESS_TAG)
    }

    /// Returns whether this is a special known `tag`, currently `Address` or `Identity`.
    pub fn is_special_tag(tag_name: &str) -> bool {
        tag_name == IDENTITY_TAG || tag_name == ADDRESS_TAG
    }

    /// Returns whether this is a special known type, currently `Address` or `Identity`.
    /// Does not follow `Ref`s.
    pub fn is_special(&self) -> bool {
        self.is_identity() || self.is_address()
    }

    /// Returns whether this is a unit type, that is, has no elements.
    pub fn is_unit(&self) -> bool {
        self.elements.is_empty()
    }

    /// Returns index of the field with the given `name`.
    pub fn index_of_field_name(&self, name: &str) -> Option<usize> {
        self.elements
            .iter()
            .position(|field| field.name.as_deref() == Some(name))
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
            .map(|(name, ty)| ProductTypeElement::new(ty.into(), name.map(Into::into)))
            .collect()
    }
}

impl From<Box<[ProductTypeElement]>> for ProductType {
    fn from(fields: Box<[ProductTypeElement]>) -> Self {
        ProductType::new(fields)
    }
}
impl<const N: usize> From<[ProductTypeElement; N]> for ProductType {
    fn from(fields: [ProductTypeElement; N]) -> Self {
        ProductType::new(fields.into())
    }
}
impl<const N: usize> From<[(Option<&str>, AlgebraicType); N]> for ProductType {
    fn from(fields: [(Option<&str>, AlgebraicType); N]) -> Self {
        fields.into_iter().collect()
    }
}
impl<const N: usize> From<[(&str, AlgebraicType); N]> for ProductType {
    fn from(fields: [(&str, AlgebraicType); N]) -> Self {
        fields.into_iter().collect()
    }
}
impl<const N: usize> From<[AlgebraicType; N]> for ProductType {
    fn from(fields: [AlgebraicType; N]) -> Self {
        fields.into_iter().collect()
    }
}

impl MetaType for ProductType {
    fn meta_type() -> AlgebraicType {
        AlgebraicType::product([("elements", AlgebraicType::array(ProductTypeElement::meta_type()))])
    }
}

impl ProductType {
    pub fn as_value(&self) -> AlgebraicValue {
        value_serialize(self)
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<ProductType, ValueDeserializeError> {
        Self::deserialize(ValueDeserializer::from_ref(value))
    }
}

impl<'a> WithTypespace<'a, ProductType> {
    #[inline]
    pub fn elements(&self) -> ElementsWithTypespace<'a> {
        self.iter_with(&*self.ty().elements)
    }

    #[inline]
    pub fn with_values<I: IntoIterator<Item = &'a AlgebraicValue>>(
        &self,
        vals: I,
    ) -> ElementValuesWithType<'a, I::IntoIter>
    where
        I::IntoIter: ExactSizeIterator,
    {
        let elements = self.elements();
        let vals = vals.into_iter();
        assert_eq!(elements.len(), vals.len());
        ElementValuesWithType {
            inner: std::iter::zip(elements, vals),
        }
    }
}

impl<'a> IntoIterator for WithTypespace<'a, ProductType> {
    type Item = WithTypespace<'a, ProductTypeElement>;
    type IntoIter = ElementsWithTypespace<'a>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.elements()
    }
}

pub type ElementsWithTypespace<'a> = crate::IterWithTypespace<'a, std::slice::Iter<'a, ProductTypeElement>>;

pub struct ElementValuesWithType<'a, I> {
    inner: std::iter::Zip<ElementsWithTypespace<'a>, I>,
}

impl<'a, I> Iterator for ElementValuesWithType<'a, I>
where
    I: ExactSizeIterator<Item = &'a AlgebraicValue>,
{
    type Item = ValueWithType<'a, AlgebraicValue>;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(ty, val)| ty.algebraic_type().with_value(val))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<'a, I> ExactSizeIterator for ElementValuesWithType<'a, I> where I: ExactSizeIterator<Item = &'a AlgebraicValue> {}
