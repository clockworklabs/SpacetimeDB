use crate::{algebraic_type::AlgebraicType, builtin_type::BuiltinType, meta_type::MetaType, impl_serialize};
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

impl<'de> crate::de::Deserialize<'de> for AlgebraicTypeRef {
    fn deserialize<D: crate::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        u32::deserialize(deserializer).map(Self)
    }
}

impl Display for AlgebraicTypeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // For example: `&42`.
        write!(f, "&{}", self.0)
    }
}

impl MetaType for AlgebraicTypeRef {
    fn meta_type() -> AlgebraicType {
        AlgebraicType::Builtin(BuiltinType::U32)
    }
}
