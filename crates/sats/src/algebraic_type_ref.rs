use crate::{algebraic_type::AlgebraicType, builtin_type::BuiltinType};
use std::fmt::Display;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct AlgebraicTypeRef(pub u32);

impl AlgebraicTypeRef {
    pub fn idx(self) -> usize {
        self.0 as usize
    }
}

impl crate::ser::Serialize for AlgebraicTypeRef {
    fn serialize<S: crate::ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> crate::de::Deserialize<'de> for AlgebraicTypeRef {
    fn deserialize<D: crate::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        u32::deserialize(deserializer).map(Self)
    }
}

impl Display for AlgebraicTypeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "&{}", self.0)
    }
}

impl AlgebraicTypeRef {
    pub fn make_meta_type() -> AlgebraicType {
        AlgebraicType::Builtin(BuiltinType::U32)
    }
}
