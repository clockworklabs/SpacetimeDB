use crate::{algebraic_type::AlgebraicType, builtin_type::BuiltinType};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct AlgebraicTypeRef(pub u32);

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
