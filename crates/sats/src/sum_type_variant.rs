use crate::algebraic_type::AlgebraicType;
use crate::meta_type::MetaType;
use crate::{de::Deserialize, ser::Serialize};
use crate::{static_assert_size, string, AlgebraicTypeRef, ProductTypeElement, SatsString};

/// A variant of a sum type.
///
/// NOTE: Each element has an implicit element tag based on its order.
/// Uniquely identifies an element similarly to protobuf tags.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct SumTypeVariant {
    /// The name of the variant, if any.
    pub name: Option<SatsString>,
    /// The type of the variant.
    ///
    /// Unlike a language like Rust,
    /// where we can have `enum _ { V1 { foo: A, bar: B, .. }, .. }`,
    /// the product within the variant `V1`, i.e., `{ foo: A, bar: B, .. }`
    /// is separated out in SATS into a separate product type.
    /// So we would express this as `{ V1({ foo: A, bar: B, .. }), .. }`.
    pub algebraic_type: AlgebraicType,
}

#[cfg(target_arch = "wasm32")]
static_assert_size!(SumTypeVariant, 20);
#[cfg(not(target_arch = "wasm32"))]
static_assert_size!(SumTypeVariant, 32);

impl SumTypeVariant {
    /// Returns a sum type variant with an optional `name` and `algebraic_type`.
    pub const fn new(algebraic_type: AlgebraicType, name: Option<SatsString>) -> Self {
        Self { algebraic_type, name }
    }

    /// Returns a sum type variant with `name` and `algebraic_type`.
    pub fn new_named(algebraic_type: AlgebraicType, name: &str) -> Self {
        Self::new(algebraic_type, Some(string(name)))
    }

    /// Returns a unit variant with `name`.
    pub fn unit(name: &str) -> Self {
        Self::new_named(AlgebraicType::unit(), name)
    }

    /// Returns the name of the variant.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns whether the variant has the given name.
    pub fn has_name(&self, name: &str) -> bool {
        self.name() == Some(name)
    }

    /// Returns whether this is a unit variant.
    pub fn is_unit(&self) -> bool {
        self.algebraic_type == AlgebraicType::unit()
    }
}

impl MetaType for SumTypeVariant {
    fn meta_type() -> AlgebraicType {
        let fs = [
            ProductTypeElement::new_named(AlgebraicType::option(AlgebraicType::String), "name"),
            ProductTypeElement::new_named(AlgebraicType::Ref(AlgebraicTypeRef(0)), "algebraic_type"),
        ];
        AlgebraicType::product(fs.into())
    }
}

impl From<AlgebraicType> for SumTypeVariant {
    fn from(algebraic_type: AlgebraicType) -> Self {
        Self::new(algebraic_type, None)
    }
}
