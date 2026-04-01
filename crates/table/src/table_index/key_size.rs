use crate::table_index::BytesKey;

use super::Index;
use core::mem;
use spacetimedb_memory_usage::MemoryUsage;
use spacetimedb_sats::{
    algebraic_value::Packed, i256, sum_value::SumTag, u256, AlgebraicValue, ArrayValue, ProductValue, SumValue, F32,
    F64,
};

/// Storage for memoizing `KeySize` statistics.
pub trait KeyBytesStorage: Default + MemoryUsage {
    /// Add `key.key_size_in_bytes()` to the statistics.
    fn add_to_key_bytes(&mut self, key: &(impl KeySize + ?Sized));

    /// Subtract `key.key_size_in_bytes()` from the statistics.
    fn sub_from_key_bytes(&mut self, key: &(impl KeySize + ?Sized));

    /// Resets the statistics to zero.
    fn reset_to_zero(&mut self);

    /// Returns the number bytes taken up by the keys of the index.
    fn get<I: Index>(&self, index: &I) -> u64;
}

impl KeyBytesStorage for () {
    fn add_to_key_bytes(&mut self, _: &(impl KeySize + ?Sized)) {}
    fn sub_from_key_bytes(&mut self, _: &(impl KeySize + ?Sized)) {}
    fn reset_to_zero(&mut self) {}
    fn get<I: Index>(&self, index: &I) -> u64 {
        index.num_keys() as u64 * mem::size_of::<I::Key>() as u64
    }
}

impl KeyBytesStorage for u64 {
    fn add_to_key_bytes(&mut self, key: &(impl KeySize + ?Sized)) {
        *self += key.key_size_in_bytes() as u64;
    }
    fn sub_from_key_bytes(&mut self, key: &(impl KeySize + ?Sized)) {
        *self -= key.key_size_in_bytes() as u64;
    }
    fn reset_to_zero(&mut self) {
        *self = 0;
    }
    fn get<I: Index>(&self, _: &I) -> u64 {
        *self
    }
}

/// Index keys whose memory usage we can measure and report.
///
/// The reported memory usage of an index is based on:
///
/// - the number of entries in that index, i.e. the number of `RowPointer`s it stores,
/// - the total size of the keys for every entry in that index.
///
/// This trait is used to measure the latter.
/// The metric we measure, sometimes called "data size,"
/// is the number of live user-supplied bytes in the key.
/// This excludes padding and lengths, though it does include sum tags.
///
/// The key size of a value is defined depending on that value's type:
/// - Integer, float and boolean values take bytes according to their [`std::mem::size_of`].
/// - Strings take bytes equal to their length in bytes.
///   No overhead is counted, unlike in the BFLATN or BSATN size.
/// - Sum values take 1 byte for the tag, plus the bytes of their active payload.
///   Inactive variants and padding are not counted, unlike in the BFLATN size.
/// - Product values take bytes equal to the sum of their elements' bytes.
///   Padding is not counted, unlike in the BFLATN size.
/// - Array values take bytes equal to the sum of their elements' bytes.
///   As with strings, no overhead is counted.
pub trait KeySize {
    type MemoStorage: KeyBytesStorage;

    /// Returns the size of this key in bytes.
    ///
    /// The default implementation returns the static size of the type,
    /// which is correct for primitives and composites of them.
    fn key_size_in_bytes(&self) -> usize {
        mem::size_of_val(self)
    }
}

impl<T: ?Sized + KeySize> KeySize for &T {
    type MemoStorage = T::MemoStorage;
    fn key_size_in_bytes(&self) -> usize {
        (**self).key_size_in_bytes()
    }
}

impl<T: ?Sized + KeySize> KeySize for Box<T> {
    type MemoStorage = T::MemoStorage;
    fn key_size_in_bytes(&self) -> usize {
        (**self).key_size_in_bytes()
    }
}

macro_rules! impl_key_size_primitive {
    ($prim:ty) => {
        impl KeySize for $prim {
            type MemoStorage = ();
        }
    };
    ($($prim:ty,)*) => {
        $(impl_key_size_primitive!($prim);)*
    };
}

impl_key_size_primitive!(
    bool,
    u8,
    SumTag,
    i8,
    u16,
    i16,
    u32,
    i32,
    u64,
    usize,
    i64,
    u128,
    i128,
    Packed<u128>,
    Packed<i128>,
    u256,
    i256,
    F32,
    F64,
);

impl KeySize for str {
    type MemoStorage = u64;
    fn key_size_in_bytes(&self) -> usize {
        self.len()
    }
}

impl KeySize for AlgebraicValue {
    type MemoStorage = u64;
    fn key_size_in_bytes(&self) -> usize {
        match self {
            AlgebraicValue::Bool(x) => x.key_size_in_bytes(),
            AlgebraicValue::U8(x) => x.key_size_in_bytes(),
            AlgebraicValue::I8(x) => x.key_size_in_bytes(),
            AlgebraicValue::U16(x) => x.key_size_in_bytes(),
            AlgebraicValue::I16(x) => x.key_size_in_bytes(),
            AlgebraicValue::U32(x) => x.key_size_in_bytes(),
            AlgebraicValue::I32(x) => x.key_size_in_bytes(),
            AlgebraicValue::U64(x) => x.key_size_in_bytes(),
            AlgebraicValue::I64(x) => x.key_size_in_bytes(),
            AlgebraicValue::U128(x) => x.key_size_in_bytes(),
            AlgebraicValue::I128(x) => x.key_size_in_bytes(),
            AlgebraicValue::U256(x) => x.key_size_in_bytes(),
            AlgebraicValue::I256(x) => x.key_size_in_bytes(),
            AlgebraicValue::F32(x) => x.key_size_in_bytes(),
            AlgebraicValue::F64(x) => x.key_size_in_bytes(),
            AlgebraicValue::String(x) => x.key_size_in_bytes(),
            AlgebraicValue::Sum(x) => x.key_size_in_bytes(),
            AlgebraicValue::Product(x) => x.key_size_in_bytes(),
            AlgebraicValue::Array(x) => x.key_size_in_bytes(),

            AlgebraicValue::Min | AlgebraicValue::Max => unreachable!(),
        }
    }
}

impl KeySize for SumValue {
    type MemoStorage = u64;
    fn key_size_in_bytes(&self) -> usize {
        1 + self.value.key_size_in_bytes()
    }
}

impl KeySize for ProductValue {
    type MemoStorage = u64;
    fn key_size_in_bytes(&self) -> usize {
        self.elements.key_size_in_bytes()
    }
}

impl<K> KeySize for [K]
where
    K: KeySize,
{
    type MemoStorage = u64;

    // TODO(perf, bikeshedding): check that this optimized to `size_of::<K>() * self.len()`
    // when `K` is a primitive.
    fn key_size_in_bytes(&self) -> usize {
        self.iter().map(|elt| elt.key_size_in_bytes()).sum()
    }
}

impl KeySize for ArrayValue {
    type MemoStorage = u64;

    fn key_size_in_bytes(&self) -> usize {
        match self {
            ArrayValue::Sum(elts) => elts.key_size_in_bytes(),
            ArrayValue::Product(elts) => elts.key_size_in_bytes(),
            ArrayValue::Bool(elts) => elts.key_size_in_bytes(),
            ArrayValue::I8(elts) => elts.key_size_in_bytes(),
            ArrayValue::U8(elts) => elts.key_size_in_bytes(),
            ArrayValue::I16(elts) => elts.key_size_in_bytes(),
            ArrayValue::U16(elts) => elts.key_size_in_bytes(),
            ArrayValue::I32(elts) => elts.key_size_in_bytes(),
            ArrayValue::U32(elts) => elts.key_size_in_bytes(),
            ArrayValue::I64(elts) => elts.key_size_in_bytes(),
            ArrayValue::U64(elts) => elts.key_size_in_bytes(),
            ArrayValue::I128(elts) => elts.key_size_in_bytes(),
            ArrayValue::U128(elts) => elts.key_size_in_bytes(),
            ArrayValue::I256(elts) => elts.key_size_in_bytes(),
            ArrayValue::U256(elts) => elts.key_size_in_bytes(),
            ArrayValue::F32(elts) => elts.key_size_in_bytes(),
            ArrayValue::F64(elts) => elts.key_size_in_bytes(),
            ArrayValue::String(elts) => elts.key_size_in_bytes(),
            ArrayValue::Array(elts) => elts.key_size_in_bytes(),
        }
    }
}

impl<const N: usize> KeySize for BytesKey<N> {
    type MemoStorage = ();
}
