use crate::algebraic_value::de::{ValueDeserializeError, ValueDeserializer};
use crate::algebraic_value::ser::ValueSerializer;
use crate::meta_type::MetaType;
use crate::slim_slice::SlimSliceBoxCollected;
use crate::{de::Deserialize, ser::Serialize};
use crate::{static_assert_size, string, AlgebraicType, AlgebraicValue, ProductTypeElement, SatsString, SatsVec};

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
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct ProductType {
    /// The factors of the product type.
    ///
    /// These factors can either be named or unnamed.
    /// When all the factors are unnamed, we can regard this as a plain tuple type.
    pub elements: SatsVec<ProductTypeElement>,
}

#[cfg(target_arch = "wasm32")]
static_assert_size!(ProductType, 8);
#[cfg(not(target_arch = "wasm32"))]
static_assert_size!(ProductType, 12);

impl ProductType {
    /// Returns a product type with the given `elements` as its factors.
    pub const fn new(elements: SatsVec<ProductTypeElement>) -> Self {
        Self { elements }
    }

    /// Returns whether this is the special case of `spacetimedb_lib::Identity`.
    pub fn is_identity(&self) -> bool {
        match &*self.elements {
            [ProductTypeElement {
                name: Some(name),
                algebraic_type,
            }] => name == &"__identity_bytes" && algebraic_type.is_bytes(),
            _ => false,
        }
    }
}

impl<I: Into<ProductTypeElement>> FromIterator<I> for ProductType {
    fn from_iter<T: IntoIterator<Item = I>>(iter: T) -> Self {
        Self::new(
            iter.into_iter()
                .map(Into::into)
                .collect::<SlimSliceBoxCollected<_>>()
                .unwrap(),
        )
    }
}
impl<I: Into<AlgebraicType>> FromIterator<(SatsString, I)> for ProductType {
    fn from_iter<T: IntoIterator<Item = (SatsString, I)>>(iter: T) -> Self {
        iter.into_iter().map(|(name, ty)| (Some(name), ty.into())).collect()
    }
}

impl<I: Into<AlgebraicType>> FromIterator<(Option<SatsString>, I)> for ProductType {
    fn from_iter<T: IntoIterator<Item = (Option<SatsString>, I)>>(iter: T) -> Self {
        iter.into_iter()
            .map(|(name, ty)| ProductTypeElement::new(ty.into(), name))
            .collect()
    }
}

impl<'a, I: Into<AlgebraicType>> FromIterator<(&'a str, I)> for ProductType {
    fn from_iter<T: IntoIterator<Item = (&'a str, I)>>(iter: T) -> ProductType {
        iter.into_iter().map(|(s, t)| (Some(s), t.into())).collect()
    }
}

impl<'a, I: Into<AlgebraicType>> FromIterator<(Option<&'a str>, I)> for ProductType {
    fn from_iter<T: IntoIterator<Item = (Option<&'a str>, I)>>(iter: T) -> ProductType {
        iter.into_iter().map(|(s, t)| (s.map(string), t.into())).collect()
    }
}

impl MetaType for ProductType {
    fn meta_type() -> AlgebraicType {
        let elems = ProductTypeElement::new_named(AlgebraicType::array(ProductTypeElement::meta_type()), "elements");
        AlgebraicType::product([elems].into())
    }
}

impl ProductType {
    pub fn as_value(&self) -> AlgebraicValue {
        self.serialize(ValueSerializer).expect("unexpected `len >= u32::MAX`")
    }

    pub fn from_value(value: &AlgebraicValue) -> Result<ProductType, ValueDeserializeError> {
        Self::deserialize(ValueDeserializer::from_ref(value))
    }
}
