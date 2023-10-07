use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::ValueSerializer;
use crate::meta_type::MetaType;
use crate::{de::Deserialize, ser::Serialize};
use crate::{static_assert_size, AlgebraicType, AlgebraicValue, ProductTypeElement, SatsVec, SumTypeVariant};

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
/// ```ignore
/// { N_0(T_0), N_1(T_1), ..., N_n(T_n) }
/// ```
/// is:
/// ```ignore
/// Σ (i ∈ 0..n). values(T_i)
/// ```
/// so for example, `values({ A(U64), B(Bool) }) = values(U64) + values(Bool)`.
///
/// See also: https://ncatlab.org/nlab/show/sum+type.
///
/// [structural]: https://en.wikipedia.org/wiki/Structural_type_system
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct SumType {
    /// The possible variants of the sum type.
    ///
    /// The order is relevant as it defines the tags of the variants at runtime.
    pub variants: SatsVec<SumTypeVariant>,
}

#[cfg(target_arch = "wasm32")]
static_assert_size!(SumType, 8);
#[cfg(not(target_arch = "wasm32"))]
static_assert_size!(SumType, 12);

impl SumType {
    /// Returns a sum type with these possible `variants`.
    pub const fn new(variants: SatsVec<SumTypeVariant>) -> Self {
        Self { variants }
    }

    /// Returns whether this sum type looks like an option type.
    ///
    /// An option type has `some(T)` as its first variant and `none` as its second.
    /// That is, `{ some(T), none }` or `some: T | none` depending on your notation.
    pub fn as_option(&self) -> Option<&AlgebraicType> {
        match &*self.variants {
            [first, second]
                if second.is_unit() // Done first to avoid pointer indirection when it doesn't matter.
                    && first.has_name("some")
                    && second.has_name("none") =>
            {
                Some(&first.algebraic_type)
            }
            _ => None,
        }
    }

    /// Returns whether this sum type is like on in C without data attached to the variants.
    pub fn is_simple_enum(&self) -> bool {
        self.variants.iter().all(SumTypeVariant::is_unit)
    }
}

impl MetaType for SumType {
    fn meta_type() -> AlgebraicType {
        let variants = ProductTypeElement::new_named(AlgebraicType::array(SumTypeVariant::meta_type()), "variants");
        AlgebraicType::product([variants].into())
    }
}

impl SumType {
    pub fn as_value(&self) -> AlgebraicValue {
        self.serialize(ValueSerializer).expect("unexpected `len >= u32::MAX`")
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<SumType, ValueDeserializeError> {
        Self::deserialize(ValueDeserializer::from_ref(value))
    }
}
