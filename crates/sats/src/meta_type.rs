//! Provides the `MetaType` trait.

use crate::AlgebraicType;

/// Rust types which represent components of the SATS type system
/// and can themselves be represented as algebraic objects will implement [`MetaType`].
///
/// A type's meta-type is an [`AlgebraicType`]
/// which can store the data associated with a definition of that type.
///
/// For example, the `MetaType` of [`ProductType`](crate::ProductType) is
/// ```ignore
/// AlgebraicType::product([(
///     "elements",
///     AlgebraicType::array(ProductTypeElement::meta_type()),
/// )])
/// ```
pub trait MetaType {
    /// Returns the type structure of this type as an `AlgebraicType`.
    fn meta_type() -> AlgebraicType;
}
