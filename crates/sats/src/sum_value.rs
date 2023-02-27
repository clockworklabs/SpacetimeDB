// pub mod encoding;

use crate::algebraic_value::AlgebraicValue;
use crate::sum_type::SumType;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SumValue {
    pub tag: u8,
    pub value: Box<AlgebraicValue>,
}

impl crate::Value for SumValue {
    type Type = SumType;
}
