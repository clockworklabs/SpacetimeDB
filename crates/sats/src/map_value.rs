use std::collections::BTreeMap;

use crate::algebraic_value::AlgebraicValue;
use crate::static_assert_size;

/// A map value `AlgebraicValue` → `AlgebraicValue`.
pub type MapValue = BTreeMap<AlgebraicValue, AlgebraicValue>;

static_assert_size!(MapValue, 24);

impl crate::Value for MapValue {
    type Type = crate::MapType;
}
