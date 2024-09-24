use crate::impl_st;
use crate::{algebraic_type::AlgebraicType, impl_deserialize, impl_serialize, meta_type::MetaType};
use std::fmt::Display;

/// A reference to an [`AlgebraicType`] within a `Typespace`.
///
/// Using this in a different `Typespace` than its maker
/// will most likely result in a panic.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct AlgebraicTypeRef(
    /// The index into the specific `Typespace`'s list of types.
    pub u32,
);

impl AlgebraicTypeRef {
    /// Returns the index into the specific `Typespace`'s list of types.
    pub const fn idx(self) -> usize {
        self.0 as usize
    }
}

impl_serialize!([] AlgebraicTypeRef, (self, ser) => self.0.serialize(ser));
impl_deserialize!([] AlgebraicTypeRef, de => u32::deserialize(de).map(Self));
impl_st!([] AlgebraicTypeRef, AlgebraicType::U32);

impl Display for AlgebraicTypeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // For example: `&42`.
        write!(f, "&{}", self.0)
    }
}

impl MetaType for AlgebraicTypeRef {
    fn meta_type() -> AlgebraicType {
        AlgebraicType::U32
    }
}
