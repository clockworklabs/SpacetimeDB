use spacetimedb_primitives::{ColId, ColList};

use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::value_serialize;
use crate::de::Deserialize;
use crate::meta_type::MetaType;
use crate::product_value::InvalidFieldError;
use crate::{AlgebraicType, AlgebraicValue, ProductTypeElement, SpacetimeType, ValueWithType, WithTypespace};

/// The tag used inside the special `Identity` product type.
pub const IDENTITY_TAG: &str = "__identity__";
/// The tag used inside the special `ConnectionId` product type.
pub const CONNECTION_ID_TAG: &str = "__connection_id__";
/// The tag used inside the special `Timestamp` product type.
pub const TIMESTAMP_TAG: &str = "__timestamp_micros_since_unix_epoch__";
/// The tag used inside the special `TimeDuration` product type.
pub const TIME_DURATION_TAG: &str = "__time_duration_micros__";

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
/// See also:
/// - <https://en.wikipedia.org/wiki/Record_(computer_science)>
/// - <https://ncatlab.org/nlab/show/product+type>
///
/// These structures are known as product types because the number of possible values in product
/// ```text
/// { N_0: T_0, N_1: T_1, ..., N_n: T_n }
/// ```
/// is:
/// ```text
/// Π (i ∈ 0..n). values(T_i)
/// ```
/// so for example, `values({ A: U64, B: Bool }) = values(U64) * values(Bool)`.
///
/// [structural]: https://en.wikipedia.org/wiki/Structural_type_system
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, SpacetimeType)]
#[sats(crate = crate)]
pub struct ProductType {
    /// The factors of the product type.
    ///
    /// These factors can either be named or unnamed.
    /// When all the factors are unnamed, we can regard this as a plain tuple type.
    pub elements: Box<[ProductTypeElement]>,
}

impl std::fmt::Debug for ProductType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProductType ")?;
        f.debug_map()
            .entries(
                self.elements
                    .iter()
                    .map(|elem| (crate::dbg_aggregate_name(&elem.name), &elem.algebraic_type)),
            )
            .finish()
    }
}

impl ProductType {
    /// Returns a product type with the given `elements` as its factors.
    pub const fn new(elements: Box<[ProductTypeElement]>) -> Self {
        Self { elements }
    }

    /// Returns the unit product type.
    pub fn unit() -> Self {
        Self::new([].into())
    }

    /// Returns whether this is a "newtype" with `label` and satisfying `inner`.
    /// Does not follow `Ref`s.
    fn is_newtype(&self, check: &str, inner: impl FnOnce(&AlgebraicType) -> bool) -> bool {
        match &*self.elements {
            [ProductTypeElement {
                name: Some(name),
                algebraic_type,
            }] => &**name == check && inner(algebraic_type),
            _ => false,
        }
    }

    /// Returns whether this is the special case of `spacetimedb_lib::Identity`.
    /// Does not follow `Ref`s.
    pub fn is_identity(&self) -> bool {
        self.is_newtype(IDENTITY_TAG, |i| i.is_u256())
    }

    /// Returns whether this is the special case of `spacetimedb_lib::ConnectionId`.
    /// Does not follow `Ref`s.
    pub fn is_connection_id(&self) -> bool {
        self.is_newtype(CONNECTION_ID_TAG, |i| i.is_u128())
    }

    fn is_i64_newtype(&self, expected_tag: &str) -> bool {
        match &*self.elements {
            [ProductTypeElement {
                name: Some(name),
                algebraic_type: AlgebraicType::I64,
            }] => &**name == expected_tag,
            _ => false,
        }
    }

    /// Returns whether this is the special case of `spacetimedb_lib::Timestamp`.
    /// Does not follow `Ref`s.
    pub fn is_timestamp(&self) -> bool {
        self.is_i64_newtype(TIMESTAMP_TAG)
    }

    /// Returns whether this is the special case of `spacetimedb_lib::TimeDuration`.
    /// Does not follow `Ref`s.
    pub fn is_time_duration(&self) -> bool {
        self.is_i64_newtype(TIME_DURATION_TAG)
    }

    /// Returns whether this is the special tag of `Identity`.
    pub fn is_identity_tag(tag_name: &str) -> bool {
        tag_name == IDENTITY_TAG
    }

    /// Returns whether this is the special tag of `ConnectionId`.
    pub fn is_connection_id_tag(tag_name: &str) -> bool {
        tag_name == CONNECTION_ID_TAG
    }

    /// Returns whether this is the special tag of [`crate::timestamp::Timestamp`].
    pub fn is_timestamp_tag(tag_name: &str) -> bool {
        tag_name == TIMESTAMP_TAG
    }

    /// Returns whether this is the special tag of [`crate::time_duration::TimeDuration`].
    pub fn is_time_duration_tag(tag_name: &str) -> bool {
        tag_name == TIME_DURATION_TAG
    }

    /// Returns whether this is a special known `tag`,
    /// currently `Address`, `Identity`, `Timestamp` or `TimeDuration`.
    pub fn is_special_tag(tag_name: &str) -> bool {
        [IDENTITY_TAG, CONNECTION_ID_TAG, TIMESTAMP_TAG, TIME_DURATION_TAG].contains(&tag_name)
    }

    /// Returns whether this is a special known type, currently `ConnectionId` or `Identity`.
    /// Does not follow `Ref`s.
    pub fn is_special(&self) -> bool {
        self.is_identity() || self.is_connection_id() || self.is_timestamp() || self.is_time_duration()
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

    /// This utility function is designed to project fields based on the supplied `indexes`.
    ///
    /// **Important:**
    ///
    /// The resulting [AlgebraicType] will wrap into a [ProductType] when projecting multiple
    /// (including zero) fields, otherwise it will consist of a single [AlgebraicType].
    ///
    /// **Parameters:**
    /// - `cols`: A [ColList] containing the indexes of fields to be projected.
    pub fn project(&self, cols: &ColList) -> Result<AlgebraicType, InvalidFieldError> {
        let get_field = |col_pos: ColId| {
            self.elements
                .get(col_pos.idx())
                .ok_or(InvalidFieldError { col_pos, name: None })
        };
        if let Some(head) = cols.as_singleton() {
            get_field(head).map(|f| f.algebraic_type.clone())
        } else {
            let mut fields = Vec::with_capacity(cols.len() as usize);
            for col in cols.iter() {
                fields.push(get_field(col)?.clone());
            }
            Ok(AlgebraicType::product(fields.into_boxed_slice()))
        }
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
