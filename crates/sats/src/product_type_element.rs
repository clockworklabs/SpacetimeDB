use crate::meta_type::MetaType;
use crate::{de::Deserialize, ser::Serialize};
use crate::{static_assert_size, string, AlgebraicType, AlgebraicTypeRef, SatsStr, SatsString};

/// A factor / element of a product type.
///
/// An element consist of an optional name and a type.
///
/// NOTE: Each element has an implicit element tag based on its order.
/// Uniquely identifies an element similarly to protobuf tags.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct ProductTypeElement {
    /// The name of the field / element.
    ///
    /// As our type system is structural,
    /// a type like `{ foo: U8 }`, where `foo: U8` is the `ProductTypeElement`,
    /// is inequal to `{ bar: U8 }`, although their `algebraic_type`s (`U8`) match.
    pub name: Option<SatsString>,
    /// The type of the element.
    ///
    /// Only values of this type can be stored in the element.
    pub algebraic_type: AlgebraicType,
}

#[cfg(target_arch = "wasm32")]
static_assert_size!(ProductTypeElement, 20);
#[cfg(not(target_arch = "wasm32"))]
static_assert_size!(ProductTypeElement, 32);

impl ProductTypeElement {
    /// Returns an element with the given `name` and `algebraic_type`.
    pub const fn new(algebraic_type: AlgebraicType, name: Option<SatsString>) -> Self {
        Self { algebraic_type, name }
    }

    /// Returns a named element with `name` and `algebraic_type`.
    ///
    /// Panics when `name.len() > u32::MAX`.
    pub fn new_named(algebraic_type: AlgebraicType, name: &str) -> Self {
        Self::new(algebraic_type, Some(string(name)))
    }

    /// Returns the name of the field.
    pub fn name(&self) -> Option<&SatsStr<'_>> {
        self.name.as_ref().map(|n| n.shared_ref())
    }

    /// Returns whether the field has the given name.
    pub fn has_name(&self, name: &str) -> bool {
        self.name().map(|n| &**n) == Some(name)
    }
}

impl MetaType for ProductTypeElement {
    fn meta_type() -> AlgebraicType {
        let fs = [
            Self::new_named(AlgebraicType::option(AlgebraicType::String), "name"),
            Self::new_named(AlgebraicType::Ref(AlgebraicTypeRef(0)), "algebraic_type"),
        ];
        AlgebraicType::product(fs.into())
    }
}

impl From<AlgebraicType> for ProductTypeElement {
    fn from(value: AlgebraicType) -> Self {
        ProductTypeElement::new(value, None)
    }
}
