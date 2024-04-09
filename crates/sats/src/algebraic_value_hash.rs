//! Defines hash functions for `AlgebraicValue` and friends.

use crate::{AlgebraicValue, ArrayValue, ProductValue};
use core::hash::{Hash, Hasher};

// We only manually implement those hash functions that cannot be `#[derive(Hash)]`ed.
// Those that can be are:
//
// - `sum: SumValue`: The generated impl will first write the `sum.tag: u8`,
//   and then proceed to write the `sum.value`, which delegates to our custom impl below.
//   The tag is hashed so that e.g., `Result<u32, u32>` represented as an AV
//   results in different hashes for `Ok(x)` and `Err(x)`.
//
// - `map: MapValue`: Uses the hash function for `BTreeMap<AV, AV>`,
//   which is length prefixed and then writes each `(key, value)` sequentially.
//   Eventually, this delegates to our custom impl below.
//
// - `str: Box<str>`: Uses the standard hash function for `str`.
//
// - Primitive types: Trivially what we want.

/// The hash function for an [`AlgebraicValue`] only hashes its domain types
/// and avoids length prefixing for product values.
/// This avoids the hashing `Discriminant<AlgebraicValue>`
/// which is OK as a table column will only ever have the same type (and so the same discriminant).
impl Hash for AlgebraicValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            AlgebraicValue::Sum(x) => x.hash(state),
            AlgebraicValue::Product(x) => x.hash(state),
            AlgebraicValue::Array(x) => x.hash(state),
            AlgebraicValue::Map(x) => x.hash(state),
            AlgebraicValue::Bool(x) => x.hash(state),
            AlgebraicValue::I8(x) => x.hash(state),
            AlgebraicValue::U8(x) => x.hash(state),
            AlgebraicValue::I16(x) => x.hash(state),
            AlgebraicValue::U16(x) => x.hash(state),
            AlgebraicValue::I32(x) => x.hash(state),
            AlgebraicValue::U32(x) => x.hash(state),
            AlgebraicValue::I64(x) => x.hash(state),
            AlgebraicValue::U64(x) => x.hash(state),
            AlgebraicValue::I128(x) => x.hash(state),
            AlgebraicValue::U128(x) => x.hash(state),
            AlgebraicValue::F32(x) => x.hash(state),
            AlgebraicValue::F64(x) => x.hash(state),
            AlgebraicValue::String(s) => s.hash(state),
        }
    }
}

/// The hash function for `ProductValue` does *not* length prefix.
impl Hash for ProductValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for field in self.elements.iter() {
            field.hash(state);
        }
    }
}

/// The hash function for an [`ArrayValue`] only hashes its domain types.
/// This avoids the hashing `Discriminant<ArrayValue>`
/// which is OK as a table column will only ever have the same type (and so the same discriminant).
/// The hash function will however length-prefix as the value is of variable length.
impl Hash for ArrayValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            ArrayValue::Sum(es) => es.hash(state),
            ArrayValue::Product(es) => es.hash(state),
            ArrayValue::Bool(es) => es.hash(state),
            ArrayValue::I8(es) => es.hash(state),
            ArrayValue::U8(es) => es.hash(state),
            ArrayValue::I16(es) => es.hash(state),
            ArrayValue::U16(es) => es.hash(state),
            ArrayValue::I32(es) => es.hash(state),
            ArrayValue::U32(es) => es.hash(state),
            ArrayValue::I64(es) => es.hash(state),
            ArrayValue::U64(es) => es.hash(state),
            ArrayValue::I128(es) => es.hash(state),
            ArrayValue::U128(es) => es.hash(state),
            ArrayValue::F32(es) => es.hash(state),
            ArrayValue::F64(es) => es.hash(state),
            ArrayValue::String(es) => es.hash(state),
            ArrayValue::Array(es) => es.hash(state),
            ArrayValue::Map(es) => es.hash(state),
        }
    }
}
