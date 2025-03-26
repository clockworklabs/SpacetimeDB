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

impl SumValue {
    /// Returns a new `SumValue` with the given `tag` and `value`.
    pub fn new(tag: u8, value: impl Into<AlgebraicValue>) -> Self {
        let value = Box::from(value.into());
        Self { tag, value }
    }

    /// Returns a new `SumValue` with the given `tag` and unit value.
    pub fn new_simple(tag: u8) -> Self {
        Self::new(tag, ())
    }
}

/// The tag of a `SumValue`.
/// Can be used to read out the tag of a sum value without reading the payload.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SumTag(pub u8);

impl From<SumTag> for SumValue {
    fn from(SumTag(tag): SumTag) -> Self {
        SumValue::new_simple(tag)
    }
}
