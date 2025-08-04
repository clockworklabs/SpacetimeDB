use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::value_serialize;
use crate::de::Deserialize;
use crate::meta_type::MetaType;
use crate::{AlgebraicType, AlgebraicValue, SpacetimeType, SumTypeVariant};

/// The tag used for the `Interval` variant of the special `ScheduleAt` sum type.
pub const SCHEDULE_AT_INTERVAL_TAG: &str = "Interval";
/// The tag used for the `Time` variant of the special `ScheduleAt` sum type.
pub const SCHEDULE_AT_TIME_TAG: &str = "Time";
/// The tag used for the `some` variant of the special `option` sum type.
pub const OPTION_SOME_TAG: &str = "some";
/// The tag used for the `none` variant of the special `option` sum type.
pub const OPTION_NONE_TAG: &str = "none";

/// A structural sum type.
///
/// Unlike most languages, sums in SATS are *[structural]* and not nominal.
/// When checking whether two nominal types are the same,
/// their names and/or declaration sites (e.g., module / namespace) are considered.
/// Meanwhile, a structural type system would only check the structure of the type itself,
/// e.g., the names of its variants and their inner data types in the case of a sum.
///
/// This is also known as a discriminated union (implementation) or disjoint union.
/// Another name is [coproduct (category theory)](https://ncatlab.org/nlab/show/coproduct).
///
/// These structures are known as sum types because the number of possible values a sum
/// ```text
/// { N_0(T_0), N_1(T_1), ..., N_n(T_n) }
/// ```
/// is:
/// ```text
/// Σ (i ∈ 0..n). values(T_i)
/// ```
/// so for example, `values({ A(U64), B(Bool) }) = values(U64) + values(Bool)`.
///
/// See also:
/// - <https://en.wikipedia.org/wiki/Tagged_union>
/// - <https://ncatlab.org/nlab/show/sum+type>
///
/// [structural]: https://en.wikipedia.org/wiki/Structural_type_system
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, SpacetimeType)]
#[sats(crate = crate)]
pub struct SumType {
    /// The possible variants of the sum type.
    ///
    /// The order is relevant as it defines the tags of the variants at runtime.
    pub variants: Box<[SumTypeVariant]>,
}

impl std::fmt::Debug for SumType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SumType ")?;
        f.debug_map()
            .entries(
                self.variants
                    .iter()
                    .map(|variant| (crate::dbg_aggregate_name(&variant.name), &variant.algebraic_type)),
            )
            .finish()
    }
}

impl SumType {
    /// Returns a sum type with these possible `variants`.
    pub const fn new(variants: Box<[SumTypeVariant]>) -> Self {
        Self { variants }
    }

    /// Returns a sum type of unnamed variants taken from `types`.
    pub fn new_unnamed(types: Box<[AlgebraicType]>) -> Self {
        let variants = Vec::from(types).into_iter().map(|ty| ty.into()).collect();
        Self { variants }
    }

    /// Check whether this sum type is a structural option type.
    ///
    /// A structural option type has `some(T)` as its first variant and `none` as its second.
    /// That is, `{ some(T), none }` or `some: T | none` depending on your notation.
    /// Note that `some` and `none` are lowercase, unlike Rust's `Option`.
    /// Order matters, and an option type with these variants in the opposite order will not be recognized.
    ///
    /// If the type does look like a structural option type, returns the type `T`.
    pub fn as_option(&self) -> Option<&AlgebraicType> {
        match &*self.variants {
            [first, second] if Self::are_variants_option(first, second) => Some(&first.algebraic_type),
            _ => None,
        }
    }

    /// Check whether this sum type is a structural option type.
    ///
    /// A structural option type has `some(T)` as its first variant and `none` as its second.
    /// That is, `{ some(T), none }` or `some: T | none` depending on your notation.
    /// Note that `some` and `none` are lowercase, unlike Rust's `Option`.
    /// Order matters, and an option type with these variants in the opposite order will not be recognized.
    ///
    /// If the type does look like a structural option type, returns the type `T`.
    pub fn as_option_mut(&mut self) -> Option<&mut AlgebraicType> {
        match &mut *self.variants {
            [first, second] if Self::are_variants_option(first, second) => Some(&mut first.algebraic_type),
            _ => None,
        }
    }

    fn are_variants_option(first: &SumTypeVariant, second: &SumTypeVariant) -> bool {
        second.is_unit() // Done first to avoid pointer indirection when it doesn't matter.
        && first.has_name(OPTION_SOME_TAG)
        && second.has_name(OPTION_NONE_TAG)
    }

    /// Check whether this sum type is a structural option type.
    ///
    /// A structural option type has `some(T)` as its first variant and `none` as its second.
    /// That is, `{ some(T), none }` or `some: T | none` depending on your notation.
    /// Note that `some` and `none` are lowercase, unlike Rust's `Option`.
    /// Order matters, and an option type with these variants in the opposite order will not be recognized.
    pub fn is_option(&self) -> bool {
        self.as_option().is_some()
    }

    /// Return whether this sum type is empty, that is, has no variants.
    pub fn is_empty(&self) -> bool {
        self.variants.is_empty()
    }

    /// Return whether this sum type is the special `ScheduleAt` type,
    /// `Interval(u64) | Time(u64)`.
    /// Does not follow `Ref`s.
    pub fn is_schedule_at(&self) -> bool {
        match &*self.variants {
            [first, second] => {
                first.has_name(SCHEDULE_AT_INTERVAL_TAG)
                    && first.algebraic_type.is_time_duration()
                    && second.has_name(SCHEDULE_AT_TIME_TAG)
                    && second.algebraic_type.is_timestamp()
            }
            _ => false,
        }
    }

    /// Returns whether this sum type is a special known type, currently `Option` or `ScheduleAt`.
    pub fn is_special(&self) -> bool {
        self.is_option() || self.is_schedule_at()
    }

    /// Returns whether this sum type is like on in C without data attached to the variants.
    pub fn is_simple_enum(&self) -> bool {
        self.variants.iter().all(SumTypeVariant::is_unit)
    }

    /// Returns the sum type variant using `tag_name` with their tag position.
    pub fn get_variant(&self, tag_name: &str) -> Option<(u8, &SumTypeVariant)> {
        self.variants.iter().enumerate().find_map(|(pos, x)| {
            if x.name.as_deref() == Some(tag_name) {
                Some((pos as u8, x))
            } else {
                None
            }
        })
    }

    /// Returns the sum type variant using `tag_name` with their tag position, if this is a [Self::is_simple_enum]
    pub fn get_variant_simple(&self, tag_name: &str) -> Option<(u8, &SumTypeVariant)> {
        if self.is_simple_enum() {
            self.get_variant(tag_name)
        } else {
            None
        }
    }

    /// Returns the sum type variant with the given `tag`.
    pub fn get_variant_by_tag(&self, tag: u8) -> Option<&SumTypeVariant> {
        self.variants.get(tag as usize)
    }
}

impl From<Box<[SumTypeVariant]>> for SumType {
    fn from(fields: Box<[SumTypeVariant]>) -> Self {
        SumType::new(fields)
    }
}
impl<const N: usize> From<[SumTypeVariant; N]> for SumType {
    fn from(fields: [SumTypeVariant; N]) -> Self {
        SumType::new(fields.into())
    }
}
impl<const N: usize> From<[(Option<&str>, AlgebraicType); N]> for SumType {
    fn from(fields: [(Option<&str>, AlgebraicType); N]) -> Self {
        fields.map(|(s, t)| SumTypeVariant::new(t, s.map(<_>::into))).into()
    }
}
impl<const N: usize> From<[(&str, AlgebraicType); N]> for SumType {
    fn from(fields: [(&str, AlgebraicType); N]) -> Self {
        fields.map(|(s, t)| SumTypeVariant::new_named(t, s)).into()
    }
}
impl<const N: usize> From<[AlgebraicType; N]> for SumType {
    fn from(fields: [AlgebraicType; N]) -> Self {
        fields.map(SumTypeVariant::from).into()
    }
}

impl MetaType for SumType {
    fn meta_type() -> AlgebraicType {
        AlgebraicType::product([("variants", AlgebraicType::array(SumTypeVariant::meta_type()))])
    }
}

impl SumType {
    pub fn as_value(&self) -> AlgebraicValue {
        value_serialize(self)
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<SumType, ValueDeserializeError> {
        Self::deserialize(ValueDeserializer::from_ref(value))
    }
}
