use crate::AlgebraicType;
use crate::{de::Deserialize, ser::Serialize};

/// NOTE: Each element has an implicit element tag based on its order.
/// Uniquely identifies an element similarly to protobuf tags.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[sats(crate = crate)]
pub struct ProductTypeElement {
    pub name: Option<String>,
    pub algebraic_type: AlgebraicType,
}

impl ProductTypeElement {
    pub fn new(algebraic_type: AlgebraicType, name: Option<String>) -> Self {
        Self { algebraic_type, name }
    }

    pub fn new_named(algebraic_type: AlgebraicType, name: impl Into<String>) -> Self {
        Self {
            algebraic_type,
            name: Some(name.into()),
        }
    }
}
