//! Provides the `MetaType` trait.

use crate::AlgebraicType;

/// A trait for those types which can represent their own type structure as an `AlgebraicType`.
pub trait MetaType {
    /// Returns the type structure of this type as an `AlgebraicType`.
    ///
    /// For example, if we have:
    /// ```ignore
    /// struct Foo(u32);
    /// ```
    /// then the meta type would be:
    /// ```ignore
    /// AlgebraicType::Builtin(BuiltinType::U32)
    /// ```
    fn meta_type() -> AlgebraicType;
}
