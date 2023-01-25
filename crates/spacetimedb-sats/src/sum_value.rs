pub mod encoding;
pub mod satn;

use crate::algebraic_value::AlgebraicValue;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SumValue {
    pub tag: u8,
    pub value: Box<AlgebraicValue>,
}
