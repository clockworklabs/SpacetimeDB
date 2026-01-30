//! Table indexes with specialized key types.
//!
//! Indexes could be implemented as `MultiMap<AlgebraicValue, RowPointer>` (and once were),
//! but that results in wasted memory and spurious comparisons and branches
//! because the keys must always be homogeneous at a more specific type than `AlgebraicValue`.
//!
//! As an optimization, we hoist the enum out of the keys to the index itself.
//! This is a sizeable improvement for integer keys,
//! as e.g. `u64::cmp` is much faster than `AlgebraicValue::cmp`.
//!
//! This results in some pretty ugly code, where types that would be structs
//! are instead enums with similar-looking variants for each specialized key type,
//! and methods that interact with those enums have matches with similar-looking arms.
//! Some day we may devise a better solution, but this is good enough for now.
//
// I (pgoldman 2024-02-05) suspect, but have not measured, that there's no real reason
// to have a `ProductType` variant, which would apply to multi-column indexes.
// I believe `ProductValue::cmp` to not be meaningfully faster than `AlgebraicValue::cmp`.
// Eventually, we will likely want to compile comparison functions and representations
// for `ProductValue`-keyed indexes which take advantage of type information,
// since we know when creating the index the number and type of all the indexed columns.
// This may involve a bytecode compiler, a tree of closures, or a native JIT.
///
/// We also represent unique indices more compactly than non-unique ones, avoiding the multi-map.
/// Additionally, beyond our btree indices,
/// we support direct unique indices, where key are indices into `Vec`s.
use self::hash_index::HashIndex;
use self::same_key_entry::SameKeyEntryIter;
use self::unique_direct_fixed_cap_index::{UniqueDirectFixedCapIndex, UniqueDirectFixedCapIndexRangeIter};
use self::unique_direct_index::{UniqueDirectIndex, UniqueDirectIndexPointIter, UniqueDirectIndexRangeIter};
use self::unique_hash_index::UniqueHashIndex;
use super::indexes::RowPointer;
use super::table::RowRef;
use crate::table_index::index::Despecialize;
use crate::table_index::unique_direct_index::ToFromUsize;
use crate::{read_column::ReadColumn, static_assert_size};
use core::fmt;
use core::ops::RangeBounds;
use spacetimedb_primitives::ColList;
use spacetimedb_sats::memory_usage::MemoryUsage;
use spacetimedb_sats::{
    algebraic_value::Packed, i256, product_value::InvalidFieldError, sum_value::SumTag, u256, AlgebraicType,
    AlgebraicValue, ProductType, F32, F64,
};
use spacetimedb_schema::def::IndexAlgorithm;

mod hash_index;
mod index;
mod key_size;
mod multimap;
mod same_key_entry;
pub mod unique_direct_fixed_cap_index;
pub mod unique_direct_index;
mod unique_hash_index;
pub mod uniquemap;

pub use self::index::{Index, IndexCannotSeekRange, IndexSeekRangeResult, RangedIndex};
pub use self::key_size::KeySize;

type BtreeIndex<K> = multimap::MultiMap<K>;
type BtreeIndexPointIter<'a> = SameKeyEntryIter<'a>;
type BtreeIndexRangeIter<'a, K> = multimap::MultiMapRangeIter<'a, K>;
type BtreeUniqueIndex<K> = uniquemap::UniqueMap<K>;
type BtreeUniqueIndexPointIter<'a> = uniquemap::UniqueMapPointIter<'a>;
type BtreeUniqueIndexRangeIter<'a, K> = uniquemap::UniqueMapRangeIter<'a, K>;

/// A point iterator over a [`TypedIndex`], with a specialized key type.
///
/// See module docs for info about specialization.
enum TypedIndexPointIter<'a> {
    BTree(BtreeIndexPointIter<'a>),
    UniqueBTree(BtreeUniqueIndexPointIter<'a>),
    UniqueDirect(UniqueDirectIndexPointIter),
}

impl Iterator for TypedIndexPointIter<'_> {
    type Item = RowPointer;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::BTree(this) => this.next(),
            Self::UniqueBTree(this) => this.next(),
            Self::UniqueDirect(this) => this.next(),
        }
    }
}

/// An iterator over rows matching a certain [`AlgebraicValue`] on the [`TableIndex`].
pub struct TableIndexPointIter<'a> {
    /// The iterator seeking for matching values.
    iter: TypedIndexPointIter<'a>,
}

impl Iterator for TableIndexPointIter<'_> {
    type Item = RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

/// A ranged iterator over a [`TypedIndex`], with a specialized key type.
///
/// See module docs for info about specialization.
#[derive(Clone)]
enum TypedIndexRangeIter<'a> {
    /// The range itself provided was empty.
    RangeEmpty,

    // All the non-unique btree index iterators.
    BtreeBool(BtreeIndexRangeIter<'a, bool>),
    BtreeU8(BtreeIndexRangeIter<'a, u8>),
    BtreeSumTag(BtreeIndexRangeIter<'a, SumTag>),
    BtreeI8(BtreeIndexRangeIter<'a, i8>),
    BtreeU16(BtreeIndexRangeIter<'a, u16>),
    BtreeI16(BtreeIndexRangeIter<'a, i16>),
    BtreeU32(BtreeIndexRangeIter<'a, u32>),
    BtreeI32(BtreeIndexRangeIter<'a, i32>),
    BtreeU64(BtreeIndexRangeIter<'a, u64>),
    BtreeI64(BtreeIndexRangeIter<'a, i64>),
    BtreeU128(BtreeIndexRangeIter<'a, Packed<u128>>),
    BtreeI128(BtreeIndexRangeIter<'a, Packed<i128>>),
    BtreeU256(BtreeIndexRangeIter<'a, u256>),
    BtreeI256(BtreeIndexRangeIter<'a, i256>),
    BtreeF32(BtreeIndexRangeIter<'a, F32>),
    BtreeF64(BtreeIndexRangeIter<'a, F64>),
    BtreeString(BtreeIndexRangeIter<'a, Box<str>>),
    BtreeAV(BtreeIndexRangeIter<'a, AlgebraicValue>),

    // All the unique btree index iterators.
    UniqueBtreeBool(BtreeUniqueIndexRangeIter<'a, bool>),
    UniqueBtreeU8(BtreeUniqueIndexRangeIter<'a, u8>),
    UniqueBtreeSumTag(BtreeUniqueIndexRangeIter<'a, SumTag>),
    UniqueBtreeI8(BtreeUniqueIndexRangeIter<'a, i8>),
    UniqueBtreeU16(BtreeUniqueIndexRangeIter<'a, u16>),
    UniqueBtreeI16(BtreeUniqueIndexRangeIter<'a, i16>),
    UniqueBtreeU32(BtreeUniqueIndexRangeIter<'a, u32>),
    UniqueBtreeI32(BtreeUniqueIndexRangeIter<'a, i32>),
    UniqueBtreeU64(BtreeUniqueIndexRangeIter<'a, u64>),
    UniqueBtreeI64(BtreeUniqueIndexRangeIter<'a, i64>),
    UniqueBtreeU128(BtreeUniqueIndexRangeIter<'a, Packed<u128>>),
    UniqueBtreeI128(BtreeUniqueIndexRangeIter<'a, Packed<i128>>),
    UniqueBtreeU256(BtreeUniqueIndexRangeIter<'a, u256>),
    UniqueBtreeI256(BtreeUniqueIndexRangeIter<'a, i256>),
    UniqueBtreeF32(BtreeUniqueIndexRangeIter<'a, F32>),
    UniqueBtreeF64(BtreeUniqueIndexRangeIter<'a, F64>),
    UniqueBtreeString(BtreeUniqueIndexRangeIter<'a, Box<str>>),
    UniqueBtreeAV(BtreeUniqueIndexRangeIter<'a, AlgebraicValue>),

    UniqueDirect(UniqueDirectIndexRangeIter<'a>),
    UniqueDirectU8(UniqueDirectFixedCapIndexRangeIter<'a>),
}

impl Iterator for TypedIndexRangeIter<'_> {
    type Item = RowPointer;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::RangeEmpty => None,

            Self::BtreeBool(this) => this.next(),
            Self::BtreeU8(this) => this.next(),
            Self::BtreeSumTag(this) => this.next(),
            Self::BtreeI8(this) => this.next(),
            Self::BtreeU16(this) => this.next(),
            Self::BtreeI16(this) => this.next(),
            Self::BtreeU32(this) => this.next(),
            Self::BtreeI32(this) => this.next(),
            Self::BtreeU64(this) => this.next(),
            Self::BtreeI64(this) => this.next(),
            Self::BtreeU128(this) => this.next(),
            Self::BtreeI128(this) => this.next(),
            Self::BtreeU256(this) => this.next(),
            Self::BtreeI256(this) => this.next(),
            Self::BtreeF32(this) => this.next(),
            Self::BtreeF64(this) => this.next(),
            Self::BtreeString(this) => this.next(),
            Self::BtreeAV(this) => this.next(),

            Self::UniqueBtreeBool(this) => this.next(),
            Self::UniqueBtreeU8(this) => this.next(),
            Self::UniqueBtreeSumTag(this) => this.next(),
            Self::UniqueBtreeI8(this) => this.next(),
            Self::UniqueBtreeU16(this) => this.next(),
            Self::UniqueBtreeI16(this) => this.next(),
            Self::UniqueBtreeU32(this) => this.next(),
            Self::UniqueBtreeI32(this) => this.next(),
            Self::UniqueBtreeU64(this) => this.next(),
            Self::UniqueBtreeI64(this) => this.next(),
            Self::UniqueBtreeU128(this) => this.next(),
            Self::UniqueBtreeI128(this) => this.next(),
            Self::UniqueBtreeU256(this) => this.next(),
            Self::UniqueBtreeI256(this) => this.next(),
            Self::UniqueBtreeF32(this) => this.next(),
            Self::UniqueBtreeF64(this) => this.next(),
            Self::UniqueBtreeString(this) => this.next(),
            Self::UniqueBtreeAV(this) => this.next(),

            Self::UniqueDirect(this) => this.next(),
            Self::UniqueDirectU8(this) => this.next(),
        }
    }
}

/// An iterator over rows matching a range of [`AlgebraicValue`]s on the [`TableIndex`].
#[derive(Clone)]
pub struct TableIndexRangeIter<'a> {
    /// The iterator seeking for matching values.
    iter: TypedIndexRangeIter<'a>,
}

impl Iterator for TableIndexRangeIter<'_> {
    type Item = RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl fmt::Debug for TableIndexRangeIter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let iter = self.clone();
        f.debug_list().entries(iter).finish()
    }
}

/// An index from a key type determined at runtime to `RowPointer`(s).
///
/// See module docs for info about specialization.
#[derive(Debug, PartialEq, Eq, derive_more::From)]
enum TypedIndex {
    // All the non-unique btree index types.
    BtreeBool(BtreeIndex<bool>),
    BtreeU8(BtreeIndex<u8>),
    BtreeSumTag(BtreeIndex<SumTag>),
    BtreeI8(BtreeIndex<i8>),
    BtreeU16(BtreeIndex<u16>),
    BtreeI16(BtreeIndex<i16>),
    BtreeU32(BtreeIndex<u32>),
    BtreeI32(BtreeIndex<i32>),
    BtreeU64(BtreeIndex<u64>),
    BtreeI64(BtreeIndex<i64>),
    BtreeU128(BtreeIndex<Packed<u128>>),
    BtreeI128(BtreeIndex<Packed<i128>>),
    BtreeU256(BtreeIndex<u256>),
    BtreeI256(BtreeIndex<i256>),
    BtreeF32(BtreeIndex<F32>),
    BtreeF64(BtreeIndex<F64>),
    BtreeString(BtreeIndex<Box<str>>),
    BtreeAV(BtreeIndex<AlgebraicValue>),

    // All the non-unique hash index types.
    HashBool(HashIndex<bool>),
    HashU8(HashIndex<u8>),
    HashSumTag(HashIndex<SumTag>),
    HashI8(HashIndex<i8>),
    HashU16(HashIndex<u16>),
    HashI16(HashIndex<i16>),
    HashU32(HashIndex<u32>),
    HashI32(HashIndex<i32>),
    HashU64(HashIndex<u64>),
    HashI64(HashIndex<i64>),
    HashU128(HashIndex<Packed<u128>>),
    HashI128(HashIndex<Packed<i128>>),
    HashU256(HashIndex<u256>),
    HashI256(HashIndex<i256>),
    HashF32(HashIndex<F32>),
    HashF64(HashIndex<F64>),
    HashString(HashIndex<Box<str>>),
    HashAV(HashIndex<AlgebraicValue>),

    // All the unique btree index types.
    UniqueBtreeBool(BtreeUniqueIndex<bool>),
    UniqueBtreeU8(BtreeUniqueIndex<u8>),
    UniqueBtreeSumTag(BtreeUniqueIndex<SumTag>),
    UniqueBtreeI8(BtreeUniqueIndex<i8>),
    UniqueBtreeU16(BtreeUniqueIndex<u16>),
    UniqueBtreeI16(BtreeUniqueIndex<i16>),
    UniqueBtreeU32(BtreeUniqueIndex<u32>),
    UniqueBtreeI32(BtreeUniqueIndex<i32>),
    UniqueBtreeU64(BtreeUniqueIndex<u64>),
    UniqueBtreeI64(BtreeUniqueIndex<i64>),
    UniqueBtreeU128(BtreeUniqueIndex<Packed<u128>>),
    UniqueBtreeI128(BtreeUniqueIndex<Packed<i128>>),
    UniqueBtreeU256(BtreeUniqueIndex<u256>),
    UniqueBtreeI256(BtreeUniqueIndex<i256>),
    UniqueBtreeF32(BtreeUniqueIndex<F32>),
    UniqueBtreeF64(BtreeUniqueIndex<F64>),
    UniqueBtreeString(BtreeUniqueIndex<Box<str>>),
    UniqueBtreeAV(BtreeUniqueIndex<AlgebraicValue>),

    // All the unique hash index types.
    UniqueHashBool(UniqueHashIndex<bool>),
    UniqueHashU8(UniqueHashIndex<u8>),
    UniqueHashSumTag(UniqueHashIndex<SumTag>),
    UniqueHashI8(UniqueHashIndex<i8>),
    UniqueHashU16(UniqueHashIndex<u16>),
    UniqueHashI16(UniqueHashIndex<i16>),
    UniqueHashU32(UniqueHashIndex<u32>),
    UniqueHashI32(UniqueHashIndex<i32>),
    UniqueHashU64(UniqueHashIndex<u64>),
    UniqueHashI64(UniqueHashIndex<i64>),
    UniqueHashU128(UniqueHashIndex<Packed<u128>>),
    UniqueHashI128(UniqueHashIndex<Packed<i128>>),
    UniqueHashU256(UniqueHashIndex<u256>),
    UniqueHashI256(UniqueHashIndex<i256>),
    UniqueHashF32(UniqueHashIndex<F32>),
    UniqueHashF64(UniqueHashIndex<F64>),
    UniqueHashString(UniqueHashIndex<Box<str>>),
    UniqueHashAV(UniqueHashIndex<AlgebraicValue>),

    // All the unique direct index types.
    UniqueDirectU8(UniqueDirectIndex<u8>),
    UniqueDirectSumTag(UniqueDirectFixedCapIndex<SumTag>),
    UniqueDirectU16(UniqueDirectIndex<u16>),
    UniqueDirectU32(UniqueDirectIndex<u32>),
    UniqueDirectU64(UniqueDirectIndex<u64>),
}

static_assert_size!(TypedIndex, 64);

macro_rules! same_for_all_types {
    ($scrutinee:expr, $this:ident => $body:expr) => {
        match $scrutinee {
            Self::BtreeBool($this) => $body,
            Self::BtreeU8($this) => $body,
            Self::BtreeSumTag($this) => $body,
            Self::BtreeI8($this) => $body,
            Self::BtreeU16($this) => $body,
            Self::BtreeI16($this) => $body,
            Self::BtreeU32($this) => $body,
            Self::BtreeI32($this) => $body,
            Self::BtreeU64($this) => $body,
            Self::BtreeI64($this) => $body,
            Self::BtreeU128($this) => $body,
            Self::BtreeI128($this) => $body,
            Self::BtreeU256($this) => $body,
            Self::BtreeI256($this) => $body,
            Self::BtreeF32($this) => $body,
            Self::BtreeF64($this) => $body,
            Self::BtreeString($this) => $body,
            Self::BtreeAV($this) => $body,

            Self::HashBool($this) => $body,
            Self::HashU8($this) => $body,
            Self::HashSumTag($this) => $body,
            Self::HashI8($this) => $body,
            Self::HashU16($this) => $body,
            Self::HashI16($this) => $body,
            Self::HashU32($this) => $body,
            Self::HashI32($this) => $body,
            Self::HashU64($this) => $body,
            Self::HashI64($this) => $body,
            Self::HashU128($this) => $body,
            Self::HashI128($this) => $body,
            Self::HashU256($this) => $body,
            Self::HashI256($this) => $body,
            Self::HashF32($this) => $body,
            Self::HashF64($this) => $body,
            Self::HashString($this) => $body,
            Self::HashAV($this) => $body,

            Self::UniqueBtreeBool($this) => $body,
            Self::UniqueBtreeU8($this) => $body,
            Self::UniqueBtreeSumTag($this) => $body,
            Self::UniqueBtreeI8($this) => $body,
            Self::UniqueBtreeU16($this) => $body,
            Self::UniqueBtreeI16($this) => $body,
            Self::UniqueBtreeU32($this) => $body,
            Self::UniqueBtreeI32($this) => $body,
            Self::UniqueBtreeU64($this) => $body,
            Self::UniqueBtreeI64($this) => $body,
            Self::UniqueBtreeU128($this) => $body,
            Self::UniqueBtreeI128($this) => $body,
            Self::UniqueBtreeU256($this) => $body,
            Self::UniqueBtreeI256($this) => $body,
            Self::UniqueBtreeF32($this) => $body,
            Self::UniqueBtreeF64($this) => $body,
            Self::UniqueBtreeString($this) => $body,
            Self::UniqueBtreeAV($this) => $body,

            Self::UniqueHashBool($this) => $body,
            Self::UniqueHashU8($this) => $body,
            Self::UniqueHashSumTag($this) => $body,
            Self::UniqueHashI8($this) => $body,
            Self::UniqueHashU16($this) => $body,
            Self::UniqueHashI16($this) => $body,
            Self::UniqueHashU32($this) => $body,
            Self::UniqueHashI32($this) => $body,
            Self::UniqueHashU64($this) => $body,
            Self::UniqueHashI64($this) => $body,
            Self::UniqueHashU128($this) => $body,
            Self::UniqueHashI128($this) => $body,
            Self::UniqueHashU256($this) => $body,
            Self::UniqueHashI256($this) => $body,
            Self::UniqueHashF32($this) => $body,
            Self::UniqueHashF64($this) => $body,
            Self::UniqueHashString($this) => $body,
            Self::UniqueHashAV($this) => $body,

            Self::UniqueDirectSumTag($this) => $body,
            Self::UniqueDirectU8($this) => $body,
            Self::UniqueDirectU16($this) => $body,
            Self::UniqueDirectU32($this) => $body,
            Self::UniqueDirectU64($this) => $body,
        }
    };
}

impl MemoryUsage for TypedIndex {
    fn heap_usage(&self) -> usize {
        same_for_all_types!(self, this => this.heap_usage())
    }
}

fn as_tag(av: &AlgebraicValue) -> Option<&u8> {
    av.as_sum().map(|s| &s.tag)
}

fn as_sum_tag(av: &AlgebraicValue) -> Option<&SumTag> {
    as_tag(av).map(|s| s.into())
}

#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum IndexKind {
    BTree,
    Hash,
    Direct,
}

impl IndexKind {
    pub(crate) fn from_algo(algo: &IndexAlgorithm) -> Self {
        match algo {
            IndexAlgorithm::BTree(_) => Self::BTree,
            IndexAlgorithm::Hash(_) => Self::Hash,
            IndexAlgorithm::Direct(_) => Self::Direct,
            // This is due to `#[non_exhaustive]`.
            _ => unreachable!(),
        }
    }
}

impl TypedIndex {
    /// Returns a new index with keys being of `key_type` and the index possibly `is_unique`.
    fn new(key_type: &AlgebraicType, kind: IndexKind, is_unique: bool) -> Self {
        match kind {
            IndexKind::BTree => Self::new_btree_index(key_type, is_unique),
            IndexKind::Hash => Self::new_hash_index(key_type, is_unique),
            IndexKind::Direct => Self::new_direct_index(key_type, is_unique)
                .unwrap_or_else(|| Self::new_btree_index(key_type, is_unique)),
        }
    }

    /// Returns a new direct index with key being of `key_type`.
    ///
    /// If the parameters passed are not compatible with a direct index,
    /// `None` is returned.
    fn new_direct_index(key_type: &AlgebraicType, is_unique: bool) -> Option<Self> {
        if !is_unique {
            return None;
        }

        use TypedIndex::*;
        Some(match key_type {
            AlgebraicType::U8 => UniqueDirectU8(<_>::default()),
            AlgebraicType::U16 => UniqueDirectU16(<_>::default()),
            AlgebraicType::U32 => UniqueDirectU32(<_>::default()),
            AlgebraicType::U64 => UniqueDirectU64(<_>::default()),
            // For a plain enum, use `u8` as the native type.
            AlgebraicType::Sum(sum) if sum.is_simple_enum() => {
                UniqueDirectSumTag(UniqueDirectFixedCapIndex::new(sum.variants.len()))
            }
            _ => return None,
        })
    }

    /// Returns a new btree index with key being of `key_type`.
    fn new_btree_index(key_type: &AlgebraicType, is_unique: bool) -> Self {
        use TypedIndex::*;

        // If the index is on a single column of a primitive type, string, or plain enum,
        // use a homogeneous map with a native key type.
        if is_unique {
            match key_type {
                AlgebraicType::Bool => UniqueBtreeBool(<_>::default()),
                AlgebraicType::I8 => UniqueBtreeI8(<_>::default()),
                AlgebraicType::U8 => UniqueBtreeU8(<_>::default()),
                AlgebraicType::I16 => UniqueBtreeI16(<_>::default()),
                AlgebraicType::U16 => UniqueBtreeU16(<_>::default()),
                AlgebraicType::I32 => UniqueBtreeI32(<_>::default()),
                AlgebraicType::U32 => UniqueBtreeU32(<_>::default()),
                AlgebraicType::I64 => UniqueBtreeI64(<_>::default()),
                AlgebraicType::U64 => UniqueBtreeU64(<_>::default()),
                AlgebraicType::I128 => UniqueBtreeI128(<_>::default()),
                AlgebraicType::U128 => UniqueBtreeU128(<_>::default()),
                AlgebraicType::I256 => UniqueBtreeI256(<_>::default()),
                AlgebraicType::U256 => UniqueBtreeU256(<_>::default()),
                AlgebraicType::F32 => UniqueBtreeF32(<_>::default()),
                AlgebraicType::F64 => UniqueBtreeF64(<_>::default()),
                AlgebraicType::String => UniqueBtreeString(<_>::default()),
                // For a plain enum, use `u8` as the native type.
                // We use a direct index here
                AlgebraicType::Sum(sum) if sum.is_simple_enum() => UniqueBtreeSumTag(<_>::default()),

                // The index is either multi-column,
                // or we don't care to specialize on the key type,
                // so use a map keyed on `AlgebraicValue`.
                _ => UniqueBtreeAV(<_>::default()),
            }
        } else {
            match key_type {
                AlgebraicType::Bool => BtreeBool(<_>::default()),
                AlgebraicType::I8 => BtreeI8(<_>::default()),
                AlgebraicType::U8 => BtreeU8(<_>::default()),
                AlgebraicType::I16 => BtreeI16(<_>::default()),
                AlgebraicType::U16 => BtreeU16(<_>::default()),
                AlgebraicType::I32 => BtreeI32(<_>::default()),
                AlgebraicType::U32 => BtreeU32(<_>::default()),
                AlgebraicType::I64 => BtreeI64(<_>::default()),
                AlgebraicType::U64 => BtreeU64(<_>::default()),
                AlgebraicType::I128 => BtreeI128(<_>::default()),
                AlgebraicType::U128 => BtreeU128(<_>::default()),
                AlgebraicType::I256 => BtreeI256(<_>::default()),
                AlgebraicType::U256 => BtreeU256(<_>::default()),
                AlgebraicType::F32 => BtreeF32(<_>::default()),
                AlgebraicType::F64 => BtreeF64(<_>::default()),
                AlgebraicType::String => BtreeString(<_>::default()),

                // For a plain enum, use `u8` as the native type.
                AlgebraicType::Sum(sum) if sum.is_simple_enum() => BtreeSumTag(<_>::default()),

                // The index is either multi-column,
                // or we don't care to specialize on the key type,
                // so use a map keyed on `AlgebraicValue`.
                _ => BtreeAV(<_>::default()),
            }
        }
    }

    /// Returns a new hash index with key being of `key_type`.
    fn new_hash_index(key_type: &AlgebraicType, is_unique: bool) -> Self {
        use TypedIndex::*;

        // If the index is on a single column of a primitive type, string, or plain enum,
        // use a homogeneous map with a native key type.
        if is_unique {
            match key_type {
                AlgebraicType::Bool => UniqueHashBool(<_>::default()),
                AlgebraicType::I8 => UniqueHashI8(<_>::default()),
                AlgebraicType::U8 => UniqueHashU8(<_>::default()),
                AlgebraicType::I16 => UniqueHashI16(<_>::default()),
                AlgebraicType::U16 => UniqueHashU16(<_>::default()),
                AlgebraicType::I32 => UniqueHashI32(<_>::default()),
                AlgebraicType::U32 => UniqueHashU32(<_>::default()),
                AlgebraicType::I64 => UniqueHashI64(<_>::default()),
                AlgebraicType::U64 => UniqueHashU64(<_>::default()),
                AlgebraicType::I128 => UniqueHashI128(<_>::default()),
                AlgebraicType::U128 => UniqueHashU128(<_>::default()),
                AlgebraicType::I256 => UniqueHashI256(<_>::default()),
                AlgebraicType::U256 => UniqueHashU256(<_>::default()),
                AlgebraicType::F32 => UniqueHashF32(<_>::default()),
                AlgebraicType::F64 => UniqueHashF64(<_>::default()),
                AlgebraicType::String => UniqueHashString(<_>::default()),
                // For a plain enum, use `u8` as the native type.
                // We use a direct index here
                AlgebraicType::Sum(sum) if sum.is_simple_enum() => UniqueHashSumTag(<_>::default()),

                // The index is either multi-column,
                // or we don't care to specialize on the key type,
                // so use a map keyed on `AlgebraicValue`.
                _ => UniqueHashAV(<_>::default()),
            }
        } else {
            match key_type {
                AlgebraicType::Bool => HashBool(<_>::default()),
                AlgebraicType::I8 => HashI8(<_>::default()),
                AlgebraicType::U8 => HashU8(<_>::default()),
                AlgebraicType::I16 => HashI16(<_>::default()),
                AlgebraicType::U16 => HashU16(<_>::default()),
                AlgebraicType::I32 => HashI32(<_>::default()),
                AlgebraicType::U32 => HashU32(<_>::default()),
                AlgebraicType::I64 => HashI64(<_>::default()),
                AlgebraicType::U64 => HashU64(<_>::default()),
                AlgebraicType::I128 => HashI128(<_>::default()),
                AlgebraicType::U128 => HashU128(<_>::default()),
                AlgebraicType::I256 => HashI256(<_>::default()),
                AlgebraicType::U256 => HashU256(<_>::default()),
                AlgebraicType::F32 => HashF32(<_>::default()),
                AlgebraicType::F64 => HashF64(<_>::default()),
                AlgebraicType::String => HashString(<_>::default()),

                // For a plain enum, use `u8` as the native type.
                AlgebraicType::Sum(sum) if sum.is_simple_enum() => HashSumTag(<_>::default()),

                // The index is either multi-column,
                // or we don't care to specialize on the key type,
                // so use a map keyed on `AlgebraicValue`.
                _ => HashAV(<_>::default()),
            }
        }
    }

    /// Clones the structure of this index but not the indexed elements,
    /// so the returned index is empty.
    fn clone_structure(&self) -> Self {
        same_for_all_types!(self, this => this.clone_structure().into())
    }

    /// Returns whether this is a unique index or not.
    fn is_unique(&self) -> bool {
        use TypedIndex::*;
        match self {
            BtreeBool(_) | BtreeU8(_) | BtreeSumTag(_) | BtreeI8(_) | BtreeU16(_) | BtreeI16(_) | BtreeU32(_)
            | BtreeI32(_) | BtreeU64(_) | BtreeI64(_) | BtreeU128(_) | BtreeI128(_) | BtreeU256(_) | BtreeI256(_)
            | BtreeF32(_) | BtreeF64(_) | BtreeString(_) | BtreeAV(_) | HashBool(_) | HashU8(_) | HashSumTag(_)
            | HashI8(_) | HashU16(_) | HashI16(_) | HashU32(_) | HashI32(_) | HashU64(_) | HashI64(_) | HashU128(_)
            | HashI128(_) | HashU256(_) | HashI256(_) | HashF32(_) | HashF64(_) | HashString(_) | HashAV(_) => false,
            UniqueBtreeBool(_)
            | UniqueBtreeU8(_)
            | UniqueBtreeSumTag(_)
            | UniqueBtreeI8(_)
            | UniqueBtreeU16(_)
            | UniqueBtreeI16(_)
            | UniqueBtreeU32(_)
            | UniqueBtreeI32(_)
            | UniqueBtreeU64(_)
            | UniqueBtreeI64(_)
            | UniqueBtreeU128(_)
            | UniqueBtreeI128(_)
            | UniqueBtreeU256(_)
            | UniqueBtreeI256(_)
            | UniqueBtreeF32(_)
            | UniqueBtreeF64(_)
            | UniqueBtreeString(_)
            | UniqueBtreeAV(_)
            | UniqueHashBool(_)
            | UniqueHashU8(_)
            | UniqueHashSumTag(_)
            | UniqueHashI8(_)
            | UniqueHashU16(_)
            | UniqueHashI16(_)
            | UniqueHashU32(_)
            | UniqueHashI32(_)
            | UniqueHashU64(_)
            | UniqueHashI64(_)
            | UniqueHashU128(_)
            | UniqueHashI128(_)
            | UniqueHashU256(_)
            | UniqueHashI256(_)
            | UniqueHashF32(_)
            | UniqueHashF64(_)
            | UniqueHashString(_)
            | UniqueHashAV(_)
            | UniqueDirectU8(_)
            | UniqueDirectSumTag(_)
            | UniqueDirectU16(_)
            | UniqueDirectU32(_)
            | UniqueDirectU64(_) => true,
        }
    }

    /// Add the row referred to by `row_ref` to the index `self`,
    /// which must be keyed at `cols`.
    ///
    /// Returns `Errs(existing_row)` if this index was a unique index that was violated.
    /// The index is not inserted to in that case.
    ///
    /// # Safety
    ///
    /// 1. Caller promises that `cols` matches what was given at construction (`Self::new`).
    /// 2. Caller promises that the projection of `row_ref`'s type's equals the index's key type.
    unsafe fn insert(&mut self, cols: &ColList, row_ref: RowRef<'_>) -> Result<(), RowPointer> {
        fn project_to_singleton_key<T: ReadColumn>(cols: &ColList, row_ref: RowRef<'_>) -> T {
            // Extract the column.
            let col_pos = cols.as_singleton();
            // SAFETY: Caller promised that `cols` matches what was given at construction (`Self::new`).
            // In the case of `.clone_structure()`, the structure is preserved,
            // so the promise is also preserved.
            // This entails, that because we reached here, that `cols` is singleton.
            let col_pos = unsafe { col_pos.unwrap_unchecked() }.idx();

            // Extract the layout of the column.
            let col_layouts = &row_ref.row_layout().product().elements;
            // SAFETY:
            // - Caller promised that projecting the `row_ref`'s type/layout to `self.indexed_columns`
            //   gives us the index's key type.
            //   This entails that each `ColId` in `self.indexed_columns`
            //   must be in-bounds of `row_ref`'s layout.
            let col_layout = unsafe { col_layouts.get_unchecked(col_pos) };

            // Read the column in `row_ref`.
            // SAFETY:
            // - `col_layout` was just derived from the row layout.
            // - Caller promised that the type-projection of the row type/layout
            //   equals the index's key type.
            //   We've reached here, so the index's key type is compatible with `T`.
            // - `self` is a valid row so offsetting to `col_layout` is valid.
            unsafe { T::unchecked_read_column(row_ref, col_layout) }
        }

        fn insert_at_type(
            this: &mut impl Index<Key: ReadColumn>,
            cols: &ColList,
            row_ref: RowRef<'_>,
        ) -> (Result<(), RowPointer>, Option<TypedIndex>) {
            let key = project_to_singleton_key(cols, row_ref);
            let res = this.insert(key, row_ref.pointer());
            (res, None)
        }

        /// Avoid inlining the closure into the common path.
        #[cold]
        #[inline(never)]
        fn outlined_call<R>(work: impl FnOnce() -> R) -> R {
            work()
        }

        fn direct_insert_at_type<K: ReadColumn + Ord + ToFromUsize + KeySize + core::fmt::Debug>(
            this: &mut UniqueDirectIndex<K>,
            cols: &ColList,
            row_ref: RowRef<'_>,
        ) -> (Result<(), RowPointer>, Option<TypedIndex>)
        where
            TypedIndex: From<BtreeUniqueIndex<K>>,
        {
            let key = project_to_singleton_key(cols, row_ref);
            let ptr = row_ref.pointer();
            match this.insert_maybe_despecialize(key, ptr) {
                Ok(res) => (res, None),
                Err(Despecialize) => outlined_call(|| {
                    let mut index = this.into_btree();
                    let res = index.insert(key, ptr);
                    (res, Some(index.into()))
                }),
            }
        }

        fn insert_av(
            this: &mut impl Index<Key = AlgebraicValue>,
            cols: &ColList,
            row_ref: RowRef<'_>,
        ) -> (Result<(), RowPointer>, Option<TypedIndex>) {
            // SAFETY: Caller promised that any `col` in `cols` is in-bounds of `row_ref`'s layout.
            let key = unsafe { row_ref.project_unchecked(cols) };
            let res = this.insert(key, row_ref.pointer());
            (res, None)
        }

        let (res, new) = match self {
            Self::BtreeBool(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeU8(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeSumTag(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeI8(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeU16(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeI16(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeU32(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeI32(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeU64(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeI64(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeU128(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeI128(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeU256(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeI256(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeF32(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeF64(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeString(idx) => insert_at_type(idx, cols, row_ref),
            Self::BtreeAV(idx) => insert_av(idx, cols, row_ref),
            Self::HashBool(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashU8(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashSumTag(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashI8(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashU16(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashI16(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashU32(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashI32(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashU64(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashI64(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashU128(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashI128(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashU256(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashI256(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashF32(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashF64(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashString(idx) => insert_at_type(idx, cols, row_ref),
            Self::HashAV(idx) => insert_av(idx, cols, row_ref),
            Self::UniqueBtreeBool(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeU8(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeSumTag(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeI8(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeU16(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeI16(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeU32(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeI32(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeU64(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeI64(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeU128(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeI128(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeU256(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeI256(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeF32(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeF64(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeString(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeAV(idx) => insert_av(idx, cols, row_ref),
            Self::UniqueHashBool(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashU8(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashSumTag(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashI8(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashU16(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashI16(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashU32(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashI32(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashU64(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashI64(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashU128(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashI128(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashU256(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashI256(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashF32(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashF64(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashString(idx) => insert_at_type(idx, cols, row_ref),
            Self::UniqueHashAV(this) => insert_av(this, cols, row_ref),
            Self::UniqueDirectSumTag(idx) => insert_at_type(idx, cols, row_ref),

            Self::UniqueDirectU8(idx) => direct_insert_at_type(idx, cols, row_ref),
            Self::UniqueDirectU16(idx) => direct_insert_at_type(idx, cols, row_ref),
            Self::UniqueDirectU32(idx) => direct_insert_at_type(idx, cols, row_ref),
            Self::UniqueDirectU64(idx) => direct_insert_at_type(idx, cols, row_ref),
        };

        if let Some(new) = new {
            *self = new;
        }

        res
    }

    /// Remove the row referred to by `row_ref` from the index `self`,
    /// which must be keyed at `cols`.
    ///
    /// If `cols` is inconsistent with `self`,
    /// or the `row_ref` has a row type other than that used for `self`,
    /// this will behave oddly; it may return an error, do nothing,
    /// or remove the wrong value from the index.
    /// Note, however, that it will not invoke undefined behavior.
    ///
    /// If the row was present and has been deleted, returns `Ok(true)`.
    // TODO(centril): make this unsafe and use unchecked conversions.
    fn delete(&mut self, cols: &ColList, row_ref: RowRef<'_>) -> Result<bool, InvalidFieldError> {
        fn delete_at_type<T: ReadColumn, I: Index>(
            this: &mut I,
            cols: &ColList,
            row_ref: RowRef<'_>,
            convert: impl FnOnce(T) -> I::Key,
        ) -> Result<bool, InvalidFieldError> {
            let col_pos = cols.as_singleton().unwrap();
            let key = row_ref.read_col(col_pos).map_err(|_| col_pos)?;
            let key = convert(key);
            Ok(this.delete(&key, row_ref.pointer()))
        }

        fn delete_av(
            this: &mut impl Index<Key = AlgebraicValue>,
            cols: &ColList,
            row_ref: RowRef<'_>,
        ) -> Result<bool, InvalidFieldError> {
            let key = row_ref.project(cols)?;
            Ok(this.delete(&key, row_ref.pointer()))
        }

        use core::convert::identity as id;

        match self {
            Self::BtreeBool(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeU8(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeSumTag(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeI8(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeU16(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeI16(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeU32(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeI32(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeU64(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeI64(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeU128(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeI128(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeU256(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeI256(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeF32(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeF64(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeString(this) => delete_at_type(this, cols, row_ref, id),
            Self::BtreeAV(this) => delete_av(this, cols, row_ref),
            Self::HashBool(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashU8(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashSumTag(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashI8(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashU16(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashI16(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashU32(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashI32(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashU64(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashI64(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashU128(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashI128(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashU256(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashI256(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashF32(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashF64(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashString(this) => delete_at_type(this, cols, row_ref, id),
            Self::HashAV(this) => delete_av(this, cols, row_ref),
            Self::UniqueBtreeBool(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeU8(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeSumTag(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeI8(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeU16(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeI16(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeU32(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeI32(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeU64(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeI64(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeU128(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeI128(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeU256(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeI256(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeF32(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeF64(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeString(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueBtreeAV(this) => delete_av(this, cols, row_ref),
            Self::UniqueHashBool(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashU8(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashSumTag(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashI8(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashU16(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashI16(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashU32(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashI32(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashU64(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashI64(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashU128(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashI128(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashU256(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashI256(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashF32(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashF64(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashString(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueHashAV(this) => delete_av(this, cols, row_ref),
            Self::UniqueDirectSumTag(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueDirectU8(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueDirectU16(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueDirectU32(this) => delete_at_type(this, cols, row_ref, id),
            Self::UniqueDirectU64(this) => delete_at_type(this, cols, row_ref, id),
        }
    }

    fn seek_point(&self, key: &AlgebraicValue) -> TypedIndexPointIter<'_> {
        fn iter_at_type<'a, I: Index>(
            this: &'a I,
            key: &AlgebraicValue,
            av_as_t: impl Fn(&AlgebraicValue) -> Option<&I::Key>,
        ) -> I::PointIter<'a> {
            this.seek_point(av_as_t(key).expect("key does not conform to key type of index"))
        }

        use TypedIndex::*;
        use TypedIndexPointIter::*;
        match self {
            BtreeBool(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_bool)),
            BtreeU8(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_u8)),
            BtreeSumTag(this) => BTree(iter_at_type(this, key, as_sum_tag)),
            BtreeI8(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_i8)),
            BtreeU16(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_u16)),
            BtreeI16(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_i16)),
            BtreeU32(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_u32)),
            BtreeI32(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_i32)),
            BtreeU64(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_u64)),
            BtreeI64(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_i64)),
            BtreeU128(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_u128)),
            BtreeI128(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_i128)),
            BtreeU256(this) => BTree(iter_at_type(this, key, |av| av.as_u256().map(|x| &**x))),
            BtreeI256(this) => BTree(iter_at_type(this, key, |av| av.as_i256().map(|x| &**x))),
            BtreeF32(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_f32)),
            BtreeF64(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_f64)),
            BtreeString(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_string)),
            BtreeAV(this) => BTree(this.seek_point(key)),
            HashBool(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_bool)),
            HashU8(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_u8)),
            HashSumTag(this) => BTree(iter_at_type(this, key, as_sum_tag)),
            HashI8(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_i8)),
            HashU16(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_u16)),
            HashI16(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_i16)),
            HashU32(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_u32)),
            HashI32(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_i32)),
            HashU64(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_u64)),
            HashI64(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_i64)),
            HashU128(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_u128)),
            HashI128(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_i128)),
            HashU256(this) => BTree(iter_at_type(this, key, |av| av.as_u256().map(|x| &**x))),
            HashI256(this) => BTree(iter_at_type(this, key, |av| av.as_i256().map(|x| &**x))),
            HashF32(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_f32)),
            HashF64(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_f64)),
            HashString(this) => BTree(iter_at_type(this, key, AlgebraicValue::as_string)),
            HashAV(this) => BTree(this.seek_point(key)),
            UniqueBtreeBool(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_bool)),
            UniqueBtreeU8(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_u8)),
            UniqueBtreeSumTag(this) => UniqueBTree(iter_at_type(this, key, as_sum_tag)),
            UniqueBtreeI8(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_i8)),
            UniqueBtreeU16(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_u16)),
            UniqueBtreeI16(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_i16)),
            UniqueBtreeU32(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_u32)),
            UniqueBtreeI32(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_i32)),
            UniqueBtreeU64(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_u64)),
            UniqueBtreeI64(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_i64)),
            UniqueBtreeU128(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_u128)),
            UniqueBtreeI128(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_i128)),
            UniqueBtreeU256(this) => UniqueBTree(iter_at_type(this, key, |av| av.as_u256().map(|x| &**x))),
            UniqueBtreeI256(this) => UniqueBTree(iter_at_type(this, key, |av| av.as_i256().map(|x| &**x))),
            UniqueBtreeF32(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_f32)),
            UniqueBtreeF64(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_f64)),
            UniqueBtreeString(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_string)),
            UniqueBtreeAV(this) => UniqueBTree(this.seek_point(key)),

            UniqueHashBool(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_bool)),
            UniqueHashU8(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_u8)),
            UniqueHashSumTag(this) => UniqueBTree(iter_at_type(this, key, as_sum_tag)),
            UniqueHashI8(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_i8)),
            UniqueHashU16(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_u16)),
            UniqueHashI16(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_i16)),
            UniqueHashU32(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_u32)),
            UniqueHashI32(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_i32)),
            UniqueHashU64(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_u64)),
            UniqueHashI64(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_i64)),
            UniqueHashU128(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_u128)),
            UniqueHashI128(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_i128)),
            UniqueHashU256(this) => UniqueBTree(iter_at_type(this, key, |av| av.as_u256().map(|x| &**x))),
            UniqueHashI256(this) => UniqueBTree(iter_at_type(this, key, |av| av.as_i256().map(|x| &**x))),
            UniqueHashF32(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_f32)),
            UniqueHashF64(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_f64)),
            UniqueHashString(this) => UniqueBTree(iter_at_type(this, key, AlgebraicValue::as_string)),
            UniqueHashAV(this) => UniqueBTree(this.seek_point(key)),

            UniqueDirectSumTag(this) => UniqueDirect(iter_at_type(this, key, as_sum_tag)),
            UniqueDirectU8(this) => UniqueDirect(iter_at_type(this, key, AlgebraicValue::as_u8)),
            UniqueDirectU16(this) => UniqueDirect(iter_at_type(this, key, AlgebraicValue::as_u16)),
            UniqueDirectU32(this) => UniqueDirect(iter_at_type(this, key, AlgebraicValue::as_u32)),
            UniqueDirectU64(this) => UniqueDirect(iter_at_type(this, key, AlgebraicValue::as_u64)),
        }
    }

    fn seek_range(&self, range: &impl RangeBounds<AlgebraicValue>) -> IndexSeekRangeResult<TypedIndexRangeIter<'_>> {
        // Copied from `RangeBounds::is_empty` as it's unstable.
        // TODO(centril): replace once stable.
        fn is_empty<T: PartialOrd>(bounds: &impl RangeBounds<T>) -> bool {
            use core::ops::Bound::*;
            !match (bounds.start_bound(), bounds.end_bound()) {
                (Unbounded, _) | (_, Unbounded) => true,
                (Included(start), Excluded(end))
                | (Excluded(start), Included(end))
                | (Excluded(start), Excluded(end)) => start < end,
                (Included(start), Included(end)) => start <= end,
            }
        }

        fn iter_at_type<'a, I: RangedIndex>(
            this: &'a I,
            range: &impl RangeBounds<AlgebraicValue>,
            av_as_t: impl Fn(&AlgebraicValue) -> Option<&I::Key>,
        ) -> I::RangeIter<'a> {
            let av_as_t = |v| av_as_t(v).expect("bound does not conform to key type of index");
            let start = range.start_bound().map(av_as_t);
            let end = range.end_bound().map(av_as_t);
            this.seek_range(&(start, end))
        }

        use TypedIndexRangeIter::*;
        Ok(match self {
            // Hash indices are not `RangeIndex`.
            Self::HashBool(_)
            | Self::HashU8(_)
            | Self::HashSumTag(_)
            | Self::HashI8(_)
            | Self::HashU16(_)
            | Self::HashI16(_)
            | Self::HashU32(_)
            | Self::HashI32(_)
            | Self::HashU64(_)
            | Self::HashI64(_)
            | Self::HashU128(_)
            | Self::HashI128(_)
            | Self::HashF32(_)
            | Self::HashF64(_)
            | Self::HashU256(_)
            | Self::HashI256(_)
            | Self::HashString(_)
            | Self::HashAV(_)
            | Self::UniqueHashBool(_)
            | Self::UniqueHashU8(_)
            | Self::UniqueHashSumTag(_)
            | Self::UniqueHashI8(_)
            | Self::UniqueHashU16(_)
            | Self::UniqueHashI16(_)
            | Self::UniqueHashU32(_)
            | Self::UniqueHashI32(_)
            | Self::UniqueHashU64(_)
            | Self::UniqueHashI64(_)
            | Self::UniqueHashU128(_)
            | Self::UniqueHashI128(_)
            | Self::UniqueHashF32(_)
            | Self::UniqueHashF64(_)
            | Self::UniqueHashU256(_)
            | Self::UniqueHashI256(_)
            | Self::UniqueHashString(_)
            | Self::UniqueHashAV(_) => return Err(IndexCannotSeekRange),

            // Ensure we don't panic inside `BTreeMap::seek_range`.
            _ if is_empty(range) => RangeEmpty,

            Self::BtreeBool(this) => BtreeBool(iter_at_type(this, range, AlgebraicValue::as_bool)),
            Self::BtreeU8(this) => BtreeU8(iter_at_type(this, range, AlgebraicValue::as_u8)),
            Self::BtreeSumTag(this) => BtreeSumTag(iter_at_type(this, range, as_sum_tag)),
            Self::BtreeI8(this) => BtreeI8(iter_at_type(this, range, AlgebraicValue::as_i8)),
            Self::BtreeU16(this) => BtreeU16(iter_at_type(this, range, AlgebraicValue::as_u16)),
            Self::BtreeI16(this) => BtreeI16(iter_at_type(this, range, AlgebraicValue::as_i16)),
            Self::BtreeU32(this) => BtreeU32(iter_at_type(this, range, AlgebraicValue::as_u32)),
            Self::BtreeI32(this) => BtreeI32(iter_at_type(this, range, AlgebraicValue::as_i32)),
            Self::BtreeU64(this) => BtreeU64(iter_at_type(this, range, AlgebraicValue::as_u64)),
            Self::BtreeI64(this) => BtreeI64(iter_at_type(this, range, AlgebraicValue::as_i64)),
            Self::BtreeU128(this) => BtreeU128(iter_at_type(this, range, AlgebraicValue::as_u128)),
            Self::BtreeI128(this) => BtreeI128(iter_at_type(this, range, AlgebraicValue::as_i128)),
            Self::BtreeU256(this) => BtreeU256(iter_at_type(this, range, |av| av.as_u256().map(|x| &**x))),
            Self::BtreeI256(this) => BtreeI256(iter_at_type(this, range, |av| av.as_i256().map(|x| &**x))),
            Self::BtreeF32(this) => BtreeF32(iter_at_type(this, range, AlgebraicValue::as_f32)),
            Self::BtreeF64(this) => BtreeF64(iter_at_type(this, range, AlgebraicValue::as_f64)),
            Self::BtreeString(this) => BtreeString(iter_at_type(this, range, AlgebraicValue::as_string)),
            Self::BtreeAV(this) => BtreeAV(this.seek_range(range)),

            Self::UniqueBtreeBool(this) => UniqueBtreeBool(iter_at_type(this, range, AlgebraicValue::as_bool)),
            Self::UniqueBtreeU8(this) => UniqueBtreeU8(iter_at_type(this, range, AlgebraicValue::as_u8)),
            Self::UniqueBtreeSumTag(this) => UniqueBtreeSumTag(iter_at_type(this, range, as_sum_tag)),
            Self::UniqueBtreeI8(this) => UniqueBtreeI8(iter_at_type(this, range, AlgebraicValue::as_i8)),
            Self::UniqueBtreeU16(this) => UniqueBtreeU16(iter_at_type(this, range, AlgebraicValue::as_u16)),
            Self::UniqueBtreeI16(this) => UniqueBtreeI16(iter_at_type(this, range, AlgebraicValue::as_i16)),
            Self::UniqueBtreeU32(this) => UniqueBtreeU32(iter_at_type(this, range, AlgebraicValue::as_u32)),
            Self::UniqueBtreeI32(this) => UniqueBtreeI32(iter_at_type(this, range, AlgebraicValue::as_i32)),
            Self::UniqueBtreeU64(this) => UniqueBtreeU64(iter_at_type(this, range, AlgebraicValue::as_u64)),
            Self::UniqueBtreeI64(this) => UniqueBtreeI64(iter_at_type(this, range, AlgebraicValue::as_i64)),
            Self::UniqueBtreeU128(this) => UniqueBtreeU128(iter_at_type(this, range, AlgebraicValue::as_u128)),
            Self::UniqueBtreeI128(this) => UniqueBtreeI128(iter_at_type(this, range, AlgebraicValue::as_i128)),
            Self::UniqueBtreeF32(this) => UniqueBtreeF32(iter_at_type(this, range, AlgebraicValue::as_f32)),
            Self::UniqueBtreeF64(this) => UniqueBtreeF64(iter_at_type(this, range, AlgebraicValue::as_f64)),
            Self::UniqueBtreeU256(this) => UniqueBtreeU256(iter_at_type(this, range, |av| av.as_u256().map(|x| &**x))),
            Self::UniqueBtreeI256(this) => UniqueBtreeI256(iter_at_type(this, range, |av| av.as_i256().map(|x| &**x))),
            Self::UniqueBtreeString(this) => UniqueBtreeString(iter_at_type(this, range, AlgebraicValue::as_string)),
            Self::UniqueBtreeAV(this) => UniqueBtreeAV(this.seek_range(range)),

            Self::UniqueDirectSumTag(this) => UniqueDirectU8(iter_at_type(this, range, as_sum_tag)),
            Self::UniqueDirectU8(this) => UniqueDirect(iter_at_type(this, range, AlgebraicValue::as_u8)),
            Self::UniqueDirectU16(this) => UniqueDirect(iter_at_type(this, range, AlgebraicValue::as_u16)),
            Self::UniqueDirectU32(this) => UniqueDirect(iter_at_type(this, range, AlgebraicValue::as_u32)),
            Self::UniqueDirectU64(this) => UniqueDirect(iter_at_type(this, range, AlgebraicValue::as_u64)),
        })
    }

    fn clear(&mut self) {
        same_for_all_types!(self, this => this.clear())
    }

    #[allow(unused)] // used only by tests
    fn is_empty(&self) -> bool {
        self.num_rows() == 0
    }

    /// The number of rows stored in this index.
    ///
    /// Note that, for non-unique indexes, this may be larger than [`Self::num_keys`].
    ///
    /// This method runs in constant time.
    fn num_rows(&self) -> usize {
        same_for_all_types!(self, this => this.num_rows())
    }

    fn num_keys(&self) -> usize {
        same_for_all_types!(self, this => this.num_keys())
    }

    /// The number of bytes stored in keys in this index.
    ///
    /// For non-unique indexes, duplicate keys are counted once for each row that refers to them,
    /// even though the internal storage may deduplicate them as an optimization.
    ///
    /// This method runs in constant time.
    ///
    /// See the [`KeySize`] trait for more details on how this method computes its result.
    pub fn num_key_bytes(&self) -> u64 {
        same_for_all_types!(self, this => this.num_key_bytes())
    }
}

/// An index on a set of [`ColId`]s of a table.
#[derive(Debug, PartialEq, Eq)]
pub struct TableIndex {
    /// The actual index, specialized for the appropriate key type.
    idx: TypedIndex,
    /// The key type of this index.
    /// This is the projection of the row type to the types of the columns indexed.
    // NOTE(centril): This is accessed in index scan ABIs for decoding, so don't `Box<_>` it.
    pub key_type: AlgebraicType,

    /// Given a full row, typed at some `ty: ProductType`,
    /// these columns are the ones that this index indexes.
    /// Projecting the `ty` to `self.indexed_columns` yields the index's type `self.key_type`.
    pub indexed_columns: ColList,
}

impl MemoryUsage for TableIndex {
    fn heap_usage(&self) -> usize {
        let Self {
            idx,
            key_type,
            indexed_columns,
        } = self;
        idx.heap_usage() + key_type.heap_usage() + indexed_columns.heap_usage()
    }
}

static_assert_size!(TableIndex, 96);

impl TableIndex {
    /// Returns a new possibly unique index, with `index_id` for a choice of indexing algorithm.
    pub fn new(
        row_type: &ProductType,
        indexed_columns: ColList,
        index_kind: IndexKind,
        is_unique: bool,
    ) -> Result<Self, InvalidFieldError> {
        let key_type = row_type.project(&indexed_columns)?;
        let typed_index = TypedIndex::new(&key_type, index_kind, is_unique);
        Ok(Self {
            idx: typed_index,
            key_type,
            indexed_columns,
        })
    }

    /// Clones the structure of this index but not the indexed elements,
    /// so the returned index is empty.
    pub fn clone_structure(&self) -> Self {
        let key_type = self.key_type.clone();
        let idx = self.idx.clone_structure();
        let indexed_columns = self.indexed_columns.clone();
        Self {
            idx,
            key_type,
            indexed_columns,
        }
    }

    /// Returns whether this is a unique index or not.
    pub fn is_unique(&self) -> bool {
        self.idx.is_unique()
    }

    /// Inserts `ptr` with the value `row` to this index.
    /// This index will extract the necessary values from `row` based on `self.indexed_columns`.
    ///
    /// Returns `Err(existing_row)` if this insertion would violate a unique constraint.
    ///
    /// # Safety
    ///
    /// Caller promises that projecting the `row_ref`'s type
    /// to the index's columns equals the index's key type.
    /// This is entailed by an index belonging to the table's schema.
    /// It also follows from `row_ref`'s type/layout
    /// being the same as passed in on `self`'s construction.
    pub unsafe fn check_and_insert(&mut self, row_ref: RowRef<'_>) -> Result<(), RowPointer> {
        // SAFETY:
        // 1. We're passing the same `ColList` that was provided during construction.
        // 2. Forward the caller's proof obligation.
        unsafe { self.idx.insert(&self.indexed_columns, row_ref) }
    }

    /// Deletes `row_ref` with its indexed value `row_ref.project(&self.indexed_columns)` from this index.
    ///
    /// Returns whether `ptr` was present.
    pub fn delete(&mut self, row_ref: RowRef<'_>) -> Result<bool, InvalidFieldError> {
        self.idx.delete(&self.indexed_columns, row_ref)
    }

    /// Returns whether `value` is in this index.
    pub fn contains_any(&self, value: &AlgebraicValue) -> bool {
        self.seek_point(value).next().is_some()
    }

    /// Returns the number of rows associated with this `value`.
    /// Returns `None` if 0.
    /// Returns `Some(1)` if the index is unique.
    pub fn count(&self, value: &AlgebraicValue) -> Option<usize> {
        match self.seek_point(value).count() {
            0 => None,
            n => Some(n),
        }
    }

    /// Returns an iterator that yields all the `RowPointer`s for the given `key`.
    pub fn seek_point(&self, key: &AlgebraicValue) -> TableIndexPointIter<'_> {
        TableIndexPointIter {
            iter: self.idx.seek_point(key),
        }
    }

    /// Returns an iterator over the [TableIndex],
    /// that yields all the `RowPointer`s,
    /// that fall within the specified `range`,
    /// if the index is [`RangedIndex`].
    pub fn seek_range(
        &self,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> IndexSeekRangeResult<TableIndexRangeIter<'_>> {
        Ok(TableIndexRangeIter {
            iter: self.idx.seek_range(range)?,
        })
    }

    /// Extends [`TableIndex`] with `rows`.
    ///
    /// Returns the first unique constraint violation caused when adding this index, if any.
    ///
    /// # Safety
    ///
    /// Caller promises that projecting any of the `row_ref`'s type
    /// to the index's columns equals the index's key type.
    /// This is entailed by an index belonging to the table's schema.
    /// It also follows from `row_ref`'s type/layout
    /// being the same as passed in on `self`'s construction.
    pub unsafe fn build_from_rows<'table>(
        &mut self,
        rows: impl IntoIterator<Item = RowRef<'table>>,
    ) -> Result<(), RowPointer> {
        rows.into_iter()
            // SAFETY: Forward caller proof obligation.
            .try_for_each(|row_ref| unsafe { self.check_and_insert(row_ref) })
    }

    /// Returns an error with the first unique constraint violation that
    /// would occur if `self` and `other` were to be merged.
    ///
    /// The closure `ignore` indicates whether a row in `self` should be ignored.
    pub fn can_merge(&self, other: &Self, ignore: impl Fn(&RowPointer) -> bool) -> Result<(), RowPointer> {
        use TypedIndex::*;
        match (&self.idx, &other.idx) {
            // For non-unique indices, it's always possible to merge.
            (BtreeBool(_), BtreeBool(_))
            | (BtreeU8(_), BtreeU8(_))
            | (BtreeSumTag(_), BtreeSumTag(_))
            | (BtreeI8(_), BtreeI8(_))
            | (BtreeU16(_), BtreeU16(_))
            | (BtreeI16(_), BtreeI16(_))
            | (BtreeU32(_), BtreeU32(_))
            | (BtreeI32(_), BtreeI32(_))
            | (BtreeU64(_), BtreeU64(_))
            | (BtreeI64(_), BtreeI64(_))
            | (BtreeU128(_), BtreeU128(_))
            | (BtreeI128(_), BtreeI128(_))
            | (BtreeU256(_), BtreeU256(_))
            | (BtreeI256(_), BtreeI256(_))
            | (BtreeF32(_), BtreeF32(_))
            | (BtreeF64(_), BtreeF64(_))
            | (BtreeString(_), BtreeString(_))
            | (BtreeAV(_), BtreeAV(_))
            | (HashBool(_), HashBool(_))
            | (HashU8(_), HashU8(_))
            | (HashSumTag(_), HashSumTag(_))
            | (HashI8(_), HashI8(_))
            | (HashU16(_), HashU16(_))
            | (HashI16(_), HashI16(_))
            | (HashU32(_), HashU32(_))
            | (HashI32(_), HashI32(_))
            | (HashU64(_), HashU64(_))
            | (HashI64(_), HashI64(_))
            | (HashU128(_), HashU128(_))
            | (HashI128(_), HashI128(_))
            | (HashU256(_), HashU256(_))
            | (HashI256(_), HashI256(_))
            | (HashF32(_), HashF32(_))
            | (HashF64(_), HashF64(_))
            | (HashString(_), HashString(_))
            | (HashAV(_), HashAV(_)) => Ok(()),
            // For unique indices, we'll need to see if everything in `other` can be added to `idx`.
            (UniqueBtreeBool(idx), UniqueBtreeBool(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeU8(idx), UniqueBtreeU8(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeSumTag(idx), UniqueBtreeSumTag(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeI8(idx), UniqueBtreeI8(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeU16(idx), UniqueBtreeU16(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeI16(idx), UniqueBtreeI16(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeU32(idx), UniqueBtreeU32(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeI32(idx), UniqueBtreeI32(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeU64(idx), UniqueBtreeU64(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeI64(idx), UniqueBtreeI64(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeU128(idx), UniqueBtreeU128(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeI128(idx), UniqueBtreeI128(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeU256(idx), UniqueBtreeU256(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeI256(idx), UniqueBtreeI256(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeF32(idx), UniqueBtreeF32(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeF64(idx), UniqueBtreeF64(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeString(idx), UniqueBtreeString(other)) => idx.can_merge(other, ignore),
            (UniqueBtreeAV(idx), UniqueBtreeAV(other)) => idx.can_merge(other, ignore),
            (UniqueHashBool(idx), UniqueHashBool(other)) => idx.can_merge(other, ignore),
            (UniqueHashU8(idx), UniqueHashU8(other)) => idx.can_merge(other, ignore),
            (UniqueHashSumTag(idx), UniqueHashSumTag(other)) => idx.can_merge(other, ignore),
            (UniqueHashI8(idx), UniqueHashI8(other)) => idx.can_merge(other, ignore),
            (UniqueHashU16(idx), UniqueHashU16(other)) => idx.can_merge(other, ignore),
            (UniqueHashI16(idx), UniqueHashI16(other)) => idx.can_merge(other, ignore),
            (UniqueHashU32(idx), UniqueHashU32(other)) => idx.can_merge(other, ignore),
            (UniqueHashI32(idx), UniqueHashI32(other)) => idx.can_merge(other, ignore),
            (UniqueHashU64(idx), UniqueHashU64(other)) => idx.can_merge(other, ignore),
            (UniqueHashI64(idx), UniqueHashI64(other)) => idx.can_merge(other, ignore),
            (UniqueHashU128(idx), UniqueHashU128(other)) => idx.can_merge(other, ignore),
            (UniqueHashI128(idx), UniqueHashI128(other)) => idx.can_merge(other, ignore),
            (UniqueHashU256(idx), UniqueHashU256(other)) => idx.can_merge(other, ignore),
            (UniqueHashI256(idx), UniqueHashI256(other)) => idx.can_merge(other, ignore),
            (UniqueHashF32(idx), UniqueHashF32(other)) => idx.can_merge(other, ignore),
            (UniqueHashF64(idx), UniqueHashF64(other)) => idx.can_merge(other, ignore),
            (UniqueHashString(idx), UniqueHashString(other)) => idx.can_merge(other, ignore),
            (UniqueHashAV(idx), UniqueHashAV(other)) => idx.can_merge(other, ignore),
            (UniqueDirectU8(idx), UniqueDirectU8(other)) => idx.can_merge(other, ignore),
            (UniqueDirectSumTag(idx), UniqueDirectSumTag(other)) => idx.can_merge(other, ignore),
            (UniqueDirectU16(idx), UniqueDirectU16(other)) => idx.can_merge(other, ignore),
            (UniqueDirectU32(idx), UniqueDirectU32(other)) => idx.can_merge(other, ignore),
            (UniqueDirectU64(idx), UniqueDirectU64(other)) => idx.can_merge(other, ignore),

            _ => unreachable!("non-matching index kinds"),
        }
    }

    /// Deletes all entries from the index, leaving it empty.
    ///
    /// When inserting a newly-created index into the committed state,
    /// we clear the tx state's index and insert it,
    /// rather than constructing a new `TableIndex`.
    pub fn clear(&mut self) {
        self.idx.clear();
    }

    /// The number of unique keys in this index.
    pub fn num_keys(&self) -> usize {
        self.idx.num_keys()
    }

    /// The number of rows stored in this index.
    ///
    /// Note that, for non-unique indexes, this may be larger than [`Self::num_keys`].
    ///
    /// This method runs in constant time.
    pub fn num_rows(&self) -> u64 {
        self.idx.num_rows() as u64
    }

    /// The number of bytes stored in keys in this index.
    ///
    /// For non-unique indexes, duplicate keys are counted once for each row that refers to them,
    /// even though the internal storage may deduplicate them as an optimization.
    ///
    /// This method runs in constant time.
    ///
    /// See the [`KeySize`] trait for more details on how this method computes its result.
    pub fn num_key_bytes(&self) -> u64 {
        self.idx.num_key_bytes()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::page_pool::PagePool;
    use crate::{blob_store::HashMapBlobStore, table::test::table};
    use core::ops::Bound::*;
    use decorum::Total;
    use proptest::prelude::*;
    use proptest::{
        collection::{hash_set, vec},
        test_runner::TestCaseResult,
    };
    use spacetimedb_data_structures::map::HashMap;
    use spacetimedb_lib::ProductTypeElement;
    use spacetimedb_primitives::ColId;
    use spacetimedb_sats::proptest::{generate_algebraic_value, generate_primitive_algebraic_type};
    use spacetimedb_sats::{
        product,
        proptest::{generate_product_value, generate_row_type},
        AlgebraicType, ProductType, ProductValue,
    };

    fn gen_cols(ty_len: usize) -> impl Strategy<Value = ColList> {
        vec((0..ty_len as u16).prop_map_into::<ColId>(), 1..=ty_len)
            .prop_map(|cols| cols.into_iter().collect::<ColList>())
    }

    fn gen_row_and_cols() -> impl Strategy<Value = (ProductType, ColList, ProductValue)> {
        generate_row_type(1..16).prop_flat_map(|ty| {
            (
                Just(ty.clone()),
                gen_cols(ty.elements.len()),
                generate_product_value(ty),
            )
        })
    }

    impl IndexKind {
        /// Returns a strategy generating a ranged index kind.
        fn gen_for_ranged() -> impl Strategy<Value = Self> {
            any::<bool>().prop_map(|is_direct| if is_direct { Self::Direct } else { Self::BTree })
        }
    }

    fn new_index(row_type: &ProductType, cols: &ColList, is_unique: bool, kind: IndexKind) -> TableIndex {
        TableIndex::new(row_type, cols.clone(), kind, is_unique).unwrap()
    }

    /// Extracts from `row` the relevant column values according to what columns are indexed.
    fn get_fields(cols: &ColList, row: &ProductValue) -> AlgebraicValue {
        row.project(cols).unwrap()
    }

    /// Returns whether indexing `row` again would violate a unique constraint, if any.
    fn violates_unique_constraint(index: &TableIndex, cols: &ColList, row: &ProductValue) -> bool {
        !index.is_unique() || index.contains_any(&get_fields(cols, row))
    }

    /// Returns an iterator over the rows that would violate the unique constraint of this index,
    /// if `row` were inserted,
    /// or `None`, if this index doesn't have a unique constraint.
    fn get_rows_that_violate_unique_constraint<'a>(
        index: &'a TableIndex,
        row: &'a AlgebraicValue,
    ) -> Option<TableIndexPointIter<'a>> {
        index.is_unique().then(|| index.seek_point(row))
    }

    fn successor_of_primitive(av: &AlgebraicValue) -> Option<AlgebraicValue> {
        use AlgebraicValue::*;
        match av {
            Min | Max | Sum(_) | Product(_) | Array(_) | String(_) => unimplemented!(),

            Bool(false) => Some(Bool(true)),
            Bool(true) => None,
            I8(x) => x.checked_add(1).map(I8),
            U8(x) => x.checked_add(1).map(U8),
            I16(x) => x.checked_add(1).map(I16),
            U16(x) => x.checked_add(1).map(U16),
            I32(x) => x.checked_add(1).map(I32),
            U32(x) => x.checked_add(1).map(U32),
            I64(x) => x.checked_add(1).map(I64),
            U64(x) => x.checked_add(1).map(U64),
            I128(x) => x.0.checked_add(1).map(Packed).map(I128),
            U128(x) => x.0.checked_add(1).map(Packed).map(U128),
            I256(x) => x.checked_add(1.into()).map(Box::new).map(I256),
            U256(x) => x.checked_add(1u8.into()).map(Box::new).map(U256),
            F32(x) => Some(F32(Total::from_inner(x.into_inner().next_up()))),
            F64(x) => Some(F64(Total::from_inner(x.into_inner().next_up()))),
        }
    }

    fn gen_primitive_ty_and_val() -> impl Strategy<Value = (AlgebraicType, AlgebraicValue)> {
        generate_primitive_algebraic_type().prop_flat_map(|ty| (Just(ty.clone()), generate_algebraic_value(ty)))
    }

    proptest! {
        #![proptest_config(ProptestConfig { max_shrink_iters: 0x10000000, ..Default::default() })]

        #[test]
        fn hash_index_cannot_seek_range((ty, cols, pv) in gen_row_and_cols(), is_unique: bool) {
            let index = TableIndex::new(&ty, cols.clone(), IndexKind::Hash, is_unique).unwrap();

            let key = pv.project(&cols).unwrap();
            assert_eq!(index.seek_range(&(key.clone()..=key)).unwrap_err(), IndexCannotSeekRange);
        }

        #[test]
        fn remove_nonexistent_noop((ty, cols, pv) in gen_row_and_cols(), kind: IndexKind, is_unique: bool) {
            let mut index = new_index(&ty, &cols, is_unique, kind);
            let mut table = table(ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let row_ref = table.insert(&pool, &mut blob_store, &pv).unwrap().1;
            prop_assert_eq!(index.delete(row_ref).unwrap(), false);
            prop_assert!(index.idx.is_empty());
            prop_assert_eq!(index.num_keys(), 0);
            prop_assert_eq!(index.num_key_bytes(), 0);
            prop_assert_eq!(index.num_rows(), 0);
        }

        #[test]
        fn insert_delete_noop((ty, cols, pv) in gen_row_and_cols(), kind: IndexKind, is_unique: bool) {
            let mut index = new_index(&ty, &cols, is_unique, kind);
            let mut table = table(ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let row_ref = table.insert(&pool, &mut blob_store, &pv).unwrap().1;
            let value = get_fields(&cols, &pv);

            prop_assert_eq!(index.num_keys(), 0);
            prop_assert_eq!(index.num_rows(), 0);
            prop_assert_eq!(index.contains_any(&value), false);

            prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Ok(()));
            prop_assert_eq!(index.num_keys(), 1);
            prop_assert_eq!(index.num_rows(), 1);
            prop_assert_eq!(index.contains_any(&value), true);

            prop_assert_eq!(index.delete(row_ref).unwrap(), true);
            prop_assert_eq!(index.num_keys(), 0);
            prop_assert_eq!(index.num_rows(), 0);
            prop_assert_eq!(index.contains_any(&value), false);
        }

        #[test]
        fn non_unique_allows_key_twice(
            (ty, cols, key) in gen_row_and_cols(),
            kind: IndexKind,
            vals in hash_set(any::<i32>(), 1..10)
        ) {
            // Add a field to `ty` so we can use the same key more than once.
            let mut ty = Vec::from(ty.elements);
            ty.push(ProductTypeElement::new_named(AlgebraicType::I32, "extra"));
            let ty = ProductType::from(ty.into_boxed_slice());

            let mut index = new_index(&ty, &cols, false, kind);
            let mut table = table(ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();

            let num_vals = vals.len();
            for val in vals {
                let mut key = Vec::from(key.clone().elements);
                key.push(val.into());
                let key = ProductValue::from(key);

                let row_ref = table.insert(&pool, &mut blob_store, &key).unwrap().1;

                // SAFETY: `row_ref` has the same type as was passed in when constructing `index`.
                prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Ok(()));
            }

            assert_eq!(index.num_keys(), 1);
            assert_eq!(index.num_rows() as usize, num_vals);
        }

        #[test]
        fn insert_again_violates_unique_constraint((ty, cols, pv) in gen_row_and_cols(), kind: IndexKind) {
            let mut index = new_index(&ty, &cols, true, kind);
            let mut table = table(ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let row_ref = table.insert(&pool, &mut blob_store, &pv).unwrap().1;
            let value = get_fields(&cols, &pv);

            // Nothing in the index yet.
            prop_assert_eq!(index.num_rows(), 0);
            prop_assert_eq!(violates_unique_constraint(&index, &cols, &pv), false);
            prop_assert_eq!(
                get_rows_that_violate_unique_constraint(&index, &value).unwrap().collect::<Vec<_>>(),
                []
            );

            // Insert.
            // SAFETY: `row_ref` has the same type as was passed in when constructing `index`.
            prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Ok(()));

            // Inserting again would be a problem.
            prop_assert_eq!(index.num_keys(), 1);
            prop_assert_eq!(index.num_rows(), 1);
            prop_assert_eq!(violates_unique_constraint(&index, &cols, &pv), true);
            prop_assert_eq!(
                get_rows_that_violate_unique_constraint(&index, &value).unwrap().collect::<Vec<_>>(),
                [row_ref.pointer()]
            );
            // SAFETY: `row_ref` has the same type as was passed in when constructing `index`.
            prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Err(row_ref.pointer()));
            prop_assert_eq!(index.num_keys(), 1);
            prop_assert_eq!(index.num_rows(), 1);
        }

        #[test]
        fn seek_various_ranges(needle in 1..u64::MAX, is_unique: bool, kind in IndexKind::gen_for_ranged()) {
            use AlgebraicValue::U64 as V;

            let cols = 0.into();
            let ty = ProductType::from_iter([AlgebraicType::U64]);
            let mut index = new_index(&ty, &cols, is_unique, kind);
            let mut table = table(ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();

            let prev = needle - 1;
            let next = needle + 1;
            let range = prev..=next;

            let mut val_to_ptr = HashMap::default();

            // Insert `prev`, `needle`, and `next`.
            for x in range.clone() {
                let row = product![x];
                let row_ref = table.insert(&pool, &mut blob_store, &row).unwrap().1;
                val_to_ptr.insert(x, row_ref.pointer());
                // SAFETY: `row_ref` has the same type as was passed in when constructing `index`.
                prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Ok(()));
            }

            assert_eq!(index.num_keys(), 3);
            assert_eq!(index.num_rows(), 3);
            assert_eq!(index.num_key_bytes() as usize, 3 * size_of::<u64>());

            fn test_seek(index: &TableIndex, val_to_ptr: &HashMap<u64, RowPointer>, range: impl RangeBounds<AlgebraicValue>, expect: impl IntoIterator<Item = u64>) -> TestCaseResult {
                check_seek(index.seek_range(&range).unwrap().collect(), val_to_ptr, expect)
            }

            fn check_seek(mut ptrs_in_index: Vec<RowPointer>, val_to_ptr: &HashMap<u64, RowPointer>, expect: impl IntoIterator<Item = u64>) -> TestCaseResult {
                ptrs_in_index.sort();
                let mut expected_ptrs = expect.into_iter().map(|expected| val_to_ptr.get(&expected).unwrap()).copied().collect::<Vec<_>>();
                expected_ptrs.sort();
                prop_assert_eq!(
                    ptrs_in_index,
                    expected_ptrs
                );
                Ok(())
            }

            // Test point ranges.
            for x in range.clone() {
                test_seek(&index, &val_to_ptr, V(x), [x])?;
                check_seek(index.seek_point(&V(x)).collect(), &val_to_ptr, [x])?;
            }

            // Test `..` (`RangeFull`).
            test_seek(&index, &val_to_ptr, .., [prev, needle, next])?;

            // Test `x..` (`RangeFrom`).
            test_seek(&index, &val_to_ptr, V(prev).., [prev, needle, next])?;
            test_seek(&index, &val_to_ptr, V(needle).., [needle, next])?;
            test_seek(&index, &val_to_ptr, V(next).., [next])?;

            // Test `..x` (`RangeTo`).
            test_seek(&index, &val_to_ptr, ..V(prev), [])?;
            test_seek(&index, &val_to_ptr, ..V(needle), [prev])?;
            test_seek(&index, &val_to_ptr, ..V(next), [prev, needle])?;

            // Test `..=x` (`RangeToInclusive`).
            test_seek(&index, &val_to_ptr, ..=V(prev), [prev])?;
            test_seek(&index, &val_to_ptr, ..=V(needle), [prev, needle])?;
            test_seek(&index, &val_to_ptr, ..=V(next), [prev, needle, next])?;

            // Test `x..y` (`Range`).
            test_seek(&index, &val_to_ptr, V(prev)..V(prev), [])?;
            test_seek(&index, &val_to_ptr, V(prev)..V(needle), [prev])?;
            test_seek(&index, &val_to_ptr, V(prev)..V(next), [prev, needle])?;
            test_seek(&index, &val_to_ptr, V(needle)..V(next), [needle])?;

            // Test `x..=y` (`RangeInclusive`).
            test_seek(&index, &val_to_ptr, V(prev)..=V(prev), [prev])?;
            test_seek(&index, &val_to_ptr, V(prev)..=V(needle), [prev, needle])?;
            test_seek(&index, &val_to_ptr, V(prev)..=V(next), [prev, needle, next])?;
            test_seek(&index, &val_to_ptr, V(needle)..=V(next), [needle, next])?;
            test_seek(&index, &val_to_ptr, V(next)..=V(next), [next])?;

            // Test `(x, y]` (Exclusive start, inclusive end).
            test_seek(&index, &val_to_ptr, (Excluded(V(prev)), Included(V(prev))), [])?;
            test_seek(&index, &val_to_ptr, (Excluded(V(prev)), Included(V(needle))), [needle])?;
            test_seek(&index, &val_to_ptr, (Excluded(V(prev)), Included(V(next))), [needle, next])?;

            // Test `(x, inf]` (Exclusive start, unbounded end).
            test_seek(&index, &val_to_ptr, (Excluded(V(prev)), Unbounded), [needle, next])?;
            test_seek(&index, &val_to_ptr, (Excluded(V(needle)), Unbounded), [next])?;
            test_seek(&index, &val_to_ptr, (Excluded(V(next)), Unbounded), [])?;

            // Test `(x, y)` (Exclusive start, exclusive end).
            test_seek(&index, &val_to_ptr, (Excluded(V(prev)), Excluded(V(needle))), [])?;
            test_seek(&index, &val_to_ptr, (Excluded(V(prev)), Excluded(V(next))), [needle])?;
        }

        #[test]
        fn empty_range_scans_dont_panic((ty, val) in gen_primitive_ty_and_val(), is_unique: bool, kind in IndexKind::gen_for_ranged()) {
            let succ = successor_of_primitive(&val);
            prop_assume!(succ.is_some());
            let succ = succ.unwrap();

            // Construct the index.
            let row_ty = ProductType::from([ty.clone()]);
            let mut index = new_index(&row_ty, &[0].into(), is_unique, kind);

            // Construct the table and add `val` as a row.
            let mut table = table(row_ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let pv = product![val.clone()];
            let row_ref = table.insert(&pool, &mut blob_store, &pv).unwrap().1;

            // Add the row to the index.
            assert_eq!(index.num_keys(), 0);
            assert_eq!(index.num_rows(), 0);
            assert_eq!(index.num_key_bytes(), 0);
            unsafe { index.check_and_insert(row_ref).unwrap(); }
            assert_eq!(index.num_keys(), 1);
            assert_eq!(index.num_rows(), 1);

            // Seek the empty ranges.
            let rows = index.seek_range(&(&succ..&val)).unwrap().collect::<Vec<_>>();
            assert_eq!(rows, []);
            let rows = index.seek_range(&(&succ..=&val)).unwrap().collect::<Vec<_>>();
            assert_eq!(rows, []);
            let rows = index.seek_range(&(Excluded(&succ), Included(&val))).unwrap().collect::<Vec<_>>();
            assert_eq!(rows, []);
            let rows = index.seek_range(&(Excluded(&succ), Excluded(&val))).unwrap().collect::<Vec<_>>();
            assert_eq!(rows, []);
            let rows = index.seek_range(&(Excluded(&val), Excluded(&val))).unwrap().collect::<Vec<_>>();
            assert_eq!(rows, []);
        }
    }
}
