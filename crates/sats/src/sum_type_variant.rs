use crate::algebraic_type::AlgebraicType;
use crate::meta_type::MetaType;
use crate::SpacetimeType;
use crate::raw_identifier::RawIdentifier;

/// A variant of a sum type.
///
/// NOTE: Each element has an implicit element tag based on its order.
/// Uniquely identifies an element similarly to protobuf tags.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, SpacetimeType)]
#[sats(crate = crate)]
pub struct SumTypeVariant {
    /// The name of the variant, if any.
    pub name: Option<RawIdentifier>,
    /// The type of the variant.
    ///
    /// Unlike a language like Rust,
    /// where we can have `enum _ { V1 { foo: A, bar: B, .. }, .. }`,
    /// the product within the variant `V1`, i.e., `{ foo: A, bar: B, .. }`
    /// is separated out in SATS into a separate product type.
    /// So we would express this as `{ V1({ foo: A, bar: B, .. }), .. }`.
    pub algebraic_type: AlgebraicType,
}

impl SumTypeVariant {
    /// Returns a sum type variant with an optional `name` and `algebraic_type`.
    pub const fn new(algebraic_type: AlgebraicType, name: Option<RawIdentifier>) -> Self {
        Self { algebraic_type, name }
    }

    /// Returns a sum type variant with `name` and `algebraic_type`.
    pub fn new_named(algebraic_type: AlgebraicType, name: impl AsRef<str>) -> Self {
        Self {
            algebraic_type,
            name: Some(RawIdentifier::new(name.as_ref())),
        }
    }

    /// Returns a unit variant with `name`.
    pub fn unit(name: &str) -> Self {
        Self::new_named(AlgebraicType::unit(), name)
    }

    /// Returns the name of the variant.
    pub fn name(&self) -> Option<&RawIdentifier> {
        self.name.as_ref()
    }

    /// Returns whether the variant has the given name.
    pub fn has_name(&self, name: &str) -> bool {
        self.name().is_some_and(|n| &**n == name)
    }

    /// Returns whether this is a unit variant.
    pub fn is_unit(&self) -> bool {
        self.algebraic_type == AlgebraicType::unit()
    }
}

impl MetaType for SumTypeVariant {
    fn meta_type() -> AlgebraicType {
        AlgebraicType::product([
            ("name", AlgebraicType::option(AlgebraicType::String)),
            ("algebraic_type", AlgebraicType::ZERO_REF),
        ])
    }
}

impl From<AlgebraicType> for SumTypeVariant {
    fn from(algebraic_type: AlgebraicType) -> Self {
        Self::new(algebraic_type, None)
    }
}
