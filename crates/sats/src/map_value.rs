use crate::AlgebraicValue;
use std::collections::BTreeMap;

/// A map value `AlgebraicValue` → `AlgebraicValue`.
pub type MapValue = BTreeMap<AlgebraicValue, AlgebraicValue>;

impl crate::Value for MapValue {
    type Type = crate::MapType;
}
