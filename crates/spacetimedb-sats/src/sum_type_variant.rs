use crate::algebraic_type::AlgebraicType;
use serde::{Deserialize, Serialize};

/// NOTE: Each element has an implicit element tag based on its order.
/// Uniquely identifies an element similarly to protobuf tags.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct SumTypeVariant {
    pub algebraic_type: AlgebraicType,
    pub name: Option<String>,
}

impl SumTypeVariant {
    pub fn new(algebraic_type: AlgebraicType, name: Option<String>) -> Self {
        Self { algebraic_type, name }
    }

    pub fn new_named(algebraic_type: AlgebraicType, name: impl AsRef<str>) -> Self {
        Self {
            algebraic_type,
            name: Some(name.as_ref().to_owned()),
        }
    }
}
