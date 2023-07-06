use crate::algebraic_value::AlgebraicValue;
use crate::sum_type::SumType;

/// A value of a sum type chosing a specific variant of the type.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SumValue {
    /// A tag representing the choice of one variant of the sum type's variants.
    pub tag: u8,
    /// Given a variant `Var(Ty)` in a sum type `{ Var(Ty), ... }`,
    /// this provides the `value` for `Ty`.
    pub value: Box<AlgebraicValue>,
}

impl crate::Value for SumValue {
    type Type = SumType;
}
