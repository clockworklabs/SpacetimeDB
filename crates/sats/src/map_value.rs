use std::collections::BTreeMap;
use std::mem::size_of;

use crate::algebraic_value::AlgebraicValue;
use crate::static_assert_size;

/// A map value `AlgebraicValue` → `AlgebraicValue`.
pub type MapValue = BTreeMap<AlgebraicValue, AlgebraicValue>;

static_assert_size!(MapValue, size_of::<usize>() * 3);

impl crate::Value for MapValue {
    type Type = crate::MapType;
}
