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
use super::indexes::RowPointer;
use super::table::RowRef;
use crate::{read_column::ReadColumn, static_assert_size, MemoryUsage};
use core::ops::RangeBounds;
use spacetimedb_primitives::ColList;
use spacetimedb_sats::{
    algebraic_value::Packed, i256, product_value::InvalidFieldError, sum_value::SumTag, u256, AlgebraicType,
    AlgebraicValue, ProductType,
};

mod key_size;
mod multimap;
pub mod unique_direct_fixed_cap_index;
pub mod unique_direct_index;
pub mod uniquemap;

pub use key_size::KeySize;
use spacetimedb_schema::def::IndexAlgorithm;
use unique_direct_fixed_cap_index::{UniqueDirectFixedCapIndex, UniqueDirectFixedCapIndexRangeIter};
use unique_direct_index::{UniqueDirectIndex, UniqueDirectIndexPointIter, UniqueDirectIndexRangeIter};

type BtreeIndex<K> = multimap::MultiMap<K, RowPointer>;
type BtreeIndexPointIter<'a> = multimap::MultiMapPointIter<'a, RowPointer>;
type BtreeIndexRangeIter<'a, K> = multimap::MultiMapRangeIter<'a, K, RowPointer>;
type BtreeUniqueIndex<K> = uniquemap::UniqueMap<K, RowPointer>;
type BtreeUniqueIndexPointIter<'a> = uniquemap::UniqueMapPointIter<'a, RowPointer>;
type BtreeUniqueIndexRangeIter<'a, K> = uniquemap::UniqueMapRangeIter<'a, K, RowPointer>;

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
            Self::BTree(this) => this.next().copied(),
            Self::UniqueBTree(this) => this.next().copied(),
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
enum TypedIndexRangeIter<'a> {
    // All the non-unique btree index iterators.
    BtreeBool(BtreeIndexRangeIter<'a, bool>),
    BtreeU8(BtreeIndexRangeIter<'a, u8>),
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
    BtreeString(BtreeIndexRangeIter<'a, Box<str>>),
    BtreeAV(BtreeIndexRangeIter<'a, AlgebraicValue>),

    // All the unique btree index iterators.
    UniqueBtreeBool(BtreeUniqueIndexRangeIter<'a, bool>),
    UniqueBtreeU8(BtreeUniqueIndexRangeIter<'a, u8>),
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
    UniqueBtreeString(BtreeUniqueIndexRangeIter<'a, Box<str>>),
    UniqueBtreeAV(BtreeUniqueIndexRangeIter<'a, AlgebraicValue>),

    UniqueDirect(UniqueDirectIndexRangeIter<'a>),
    UniqueDirectU8(UniqueDirectFixedCapIndexRangeIter<'a>),
}

impl Iterator for TypedIndexRangeIter<'_> {
    type Item = RowPointer;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::BtreeBool(this) => this.next().copied(),
            Self::BtreeU8(this) => this.next().copied(),
            Self::BtreeI8(this) => this.next().copied(),
            Self::BtreeU16(this) => this.next().copied(),
            Self::BtreeI16(this) => this.next().copied(),
            Self::BtreeU32(this) => this.next().copied(),
            Self::BtreeI32(this) => this.next().copied(),
            Self::BtreeU64(this) => this.next().copied(),
            Self::BtreeI64(this) => this.next().copied(),
            Self::BtreeU128(this) => this.next().copied(),
            Self::BtreeI128(this) => this.next().copied(),
            Self::BtreeU256(this) => this.next().copied(),
            Self::BtreeI256(this) => this.next().copied(),
            Self::BtreeString(this) => this.next().copied(),
            Self::BtreeAV(this) => this.next().copied(),

            Self::UniqueBtreeBool(this) => this.next().copied(),
            Self::UniqueBtreeU8(this) => this.next().copied(),
            Self::UniqueBtreeI8(this) => this.next().copied(),
            Self::UniqueBtreeU16(this) => this.next().copied(),
            Self::UniqueBtreeI16(this) => this.next().copied(),
            Self::UniqueBtreeU32(this) => this.next().copied(),
            Self::UniqueBtreeI32(this) => this.next().copied(),
            Self::UniqueBtreeU64(this) => this.next().copied(),
            Self::UniqueBtreeI64(this) => this.next().copied(),
            Self::UniqueBtreeU128(this) => this.next().copied(),
            Self::UniqueBtreeI128(this) => this.next().copied(),
            Self::UniqueBtreeU256(this) => this.next().copied(),
            Self::UniqueBtreeI256(this) => this.next().copied(),
            Self::UniqueBtreeString(this) => this.next().copied(),
            Self::UniqueBtreeAV(this) => this.next().copied(),

            Self::UniqueDirect(this) => this.next(),
            Self::UniqueDirectU8(this) => this.next(),
        }
    }
}

/// An iterator over rows matching a range of [`AlgebraicValue`]s on the [`TableIndex`].
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

/// An index from a key type determined at runtime to `RowPointer`(s).
///
/// See module docs for info about specialization.
#[derive(Debug, PartialEq, Eq)]
enum TypedIndex {
    // All the non-unique btree index types.
    BtreeBool(BtreeIndex<bool>),
    BtreeU8(BtreeIndex<u8>),
    BtreeSumTag(BtreeIndex<u8>),
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
    BtreeString(BtreeIndex<Box<str>>),
    BtreeAV(BtreeIndex<AlgebraicValue>),

    // All the unique btree index types.
    UniqueBtreeBool(BtreeUniqueIndex<bool>),
    UniqueBtreeU8(BtreeUniqueIndex<u8>),
    UniqueBtreeSumTag(BtreeUniqueIndex<u8>),
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
    UniqueBtreeString(BtreeUniqueIndex<Box<str>>),
    UniqueBtreeAV(BtreeUniqueIndex<AlgebraicValue>),

    // All the unique direct index types.
    UniqueDirectU8(UniqueDirectIndex),
    UniqueDirectSumTag(UniqueDirectFixedCapIndex),
    UniqueDirectU16(UniqueDirectIndex),
    UniqueDirectU32(UniqueDirectIndex),
    UniqueDirectU64(UniqueDirectIndex),
}

impl MemoryUsage for TypedIndex {
    fn heap_usage(&self) -> usize {
        match self {
            TypedIndex::BtreeBool(this) => this.heap_usage(),
            TypedIndex::BtreeU8(this) | TypedIndex::BtreeSumTag(this) => this.heap_usage(),
            TypedIndex::BtreeI8(this) => this.heap_usage(),
            TypedIndex::BtreeU16(this) => this.heap_usage(),
            TypedIndex::BtreeI16(this) => this.heap_usage(),
            TypedIndex::BtreeU32(this) => this.heap_usage(),
            TypedIndex::BtreeI32(this) => this.heap_usage(),
            TypedIndex::BtreeU64(this) => this.heap_usage(),
            TypedIndex::BtreeI64(this) => this.heap_usage(),
            TypedIndex::BtreeU128(this) => this.heap_usage(),
            TypedIndex::BtreeI128(this) => this.heap_usage(),
            TypedIndex::BtreeU256(this) => this.heap_usage(),
            TypedIndex::BtreeI256(this) => this.heap_usage(),
            TypedIndex::BtreeString(this) => this.heap_usage(),
            TypedIndex::BtreeAV(this) => this.heap_usage(),

            TypedIndex::UniqueBtreeBool(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeU8(this) | TypedIndex::UniqueBtreeSumTag(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeI8(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeU16(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeI16(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeU32(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeI32(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeU64(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeI64(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeU128(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeI128(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeU256(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeI256(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeString(this) => this.heap_usage(),
            TypedIndex::UniqueBtreeAV(this) => this.heap_usage(),

            TypedIndex::UniqueDirectSumTag(this) => this.heap_usage(),
            TypedIndex::UniqueDirectU8(this)
            | TypedIndex::UniqueDirectU16(this)
            | TypedIndex::UniqueDirectU32(this)
            | TypedIndex::UniqueDirectU64(this) => this.heap_usage(),
        }
    }
}

fn as_tag(av: &AlgebraicValue) -> Option<&u8> {
    av.as_sum().map(|s| &s.tag)
}

impl TypedIndex {
    /// Returns a new index with keys being of `key_type` and the index possibly `is_unique`.
    fn new(key_type: &AlgebraicType, index_algo: &IndexAlgorithm, is_unique: bool) -> Self {
        use TypedIndex::*;

        if let IndexAlgorithm::Direct(_) = index_algo {
            assert!(is_unique);
            return match key_type {
                AlgebraicType::U8 => Self::UniqueDirectU8(<_>::default()),
                AlgebraicType::U16 => Self::UniqueDirectU16(<_>::default()),
                AlgebraicType::U32 => Self::UniqueDirectU32(<_>::default()),
                AlgebraicType::U64 => Self::UniqueDirectU64(<_>::default()),
                // For a plain enum, use `u8` as the native type.
                AlgebraicType::Sum(sum) if sum.is_simple_enum() => {
                    UniqueDirectSumTag(UniqueDirectFixedCapIndex::new(sum.variants.len()))
                }
                _ => unreachable!("unexpected key type {key_type:?} for direct index"),
            };
        }

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

    /// Clones the structure of this index but not the indexed elements,
    /// so the returned index is empty.
    fn clone_structure(&self) -> Self {
        use TypedIndex::*;
        match self {
            BtreeBool(_) => BtreeBool(<_>::default()),
            BtreeU8(_) => BtreeU8(<_>::default()),
            BtreeSumTag(_) => BtreeSumTag(<_>::default()),
            BtreeI8(_) => BtreeI8(<_>::default()),
            BtreeU16(_) => BtreeU16(<_>::default()),
            BtreeI16(_) => BtreeI16(<_>::default()),
            BtreeU32(_) => BtreeU32(<_>::default()),
            BtreeI32(_) => BtreeI32(<_>::default()),
            BtreeU64(_) => BtreeU64(<_>::default()),
            BtreeI64(_) => BtreeI64(<_>::default()),
            BtreeU128(_) => BtreeU128(<_>::default()),
            BtreeI128(_) => BtreeI128(<_>::default()),
            BtreeU256(_) => BtreeU256(<_>::default()),
            BtreeI256(_) => BtreeI256(<_>::default()),
            BtreeString(_) => BtreeString(<_>::default()),
            BtreeAV(_) => BtreeAV(<_>::default()),
            UniqueBtreeBool(_) => UniqueBtreeBool(<_>::default()),
            UniqueBtreeU8(_) => UniqueBtreeU8(<_>::default()),
            UniqueBtreeSumTag(_) => UniqueBtreeSumTag(<_>::default()),
            UniqueBtreeI8(_) => UniqueBtreeI8(<_>::default()),
            UniqueBtreeU16(_) => UniqueBtreeU16(<_>::default()),
            UniqueBtreeI16(_) => UniqueBtreeI16(<_>::default()),
            UniqueBtreeU32(_) => UniqueBtreeU32(<_>::default()),
            UniqueBtreeI32(_) => UniqueBtreeI32(<_>::default()),
            UniqueBtreeU64(_) => UniqueBtreeU64(<_>::default()),
            UniqueBtreeI64(_) => UniqueBtreeI64(<_>::default()),
            UniqueBtreeU128(_) => UniqueBtreeU128(<_>::default()),
            UniqueBtreeI128(_) => UniqueBtreeI128(<_>::default()),
            UniqueBtreeU256(_) => UniqueBtreeU256(<_>::default()),
            UniqueBtreeI256(_) => UniqueBtreeI256(<_>::default()),
            UniqueBtreeString(_) => UniqueBtreeString(<_>::default()),
            UniqueBtreeAV(_) => UniqueBtreeAV(<_>::default()),
            UniqueDirectU8(_) => UniqueDirectU8(<_>::default()),
            UniqueDirectSumTag(idx) => UniqueDirectSumTag(idx.clone_structure()),
            UniqueDirectU16(_) => UniqueDirectU16(<_>::default()),
            UniqueDirectU32(_) => UniqueDirectU32(<_>::default()),
            UniqueDirectU64(_) => UniqueDirectU64(<_>::default()),
        }
    }

    /// Returns whether this is a unique index or not.
    fn is_unique(&self) -> bool {
        use TypedIndex::*;
        match self {
            BtreeBool(_) | BtreeU8(_) | BtreeSumTag(_) | BtreeI8(_) | BtreeU16(_) | BtreeI16(_) | BtreeU32(_)
            | BtreeI32(_) | BtreeU64(_) | BtreeI64(_) | BtreeU128(_) | BtreeI128(_) | BtreeU256(_) | BtreeI256(_)
            | BtreeString(_) | BtreeAV(_) => false,
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
            | UniqueBtreeString(_)
            | UniqueBtreeAV(_)
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
    /// The returned `usize` is the number of bytes used by the key.
    /// [`TableIndex::check_and_insert`] will use this
    /// to update the counter for [`TableIndex::num_key_bytes`].
    /// We want to store said counter outside of the [`TypedIndex`] enum,
    /// but we can only compute the size using type info within the [`TypedIndex`],
    /// so we have to return the size across this boundary.
    ///
    /// Returns `Errs(existing_row)` if this index was a unique index that was violated.
    /// The index is not inserted to in that case.
    ///
    /// # Safety
    ///
    /// 1. Caller promises that `cols` matches what was given at construction (`Self::new`).
    /// 2. Caller promises that the projection of `row_ref`'s type's equals the index's key type.
    unsafe fn insert(&mut self, cols: &ColList, row_ref: RowRef<'_>) -> Result<usize, RowPointer> {
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

        fn mm_insert_at_type<T: Ord + ReadColumn + KeySize>(
            this: &mut BtreeIndex<T>,
            cols: &ColList,
            row_ref: RowRef<'_>,
        ) -> Result<usize, RowPointer> {
            let key: T = project_to_singleton_key(cols, row_ref);
            let key_size = key.key_size_in_bytes();
            this.insert(key, row_ref.pointer());
            Ok(key_size)
        }
        fn um_insert_at_type<T: Ord + ReadColumn + KeySize>(
            this: &mut BtreeUniqueIndex<T>,
            cols: &ColList,
            row_ref: RowRef<'_>,
        ) -> Result<usize, RowPointer> {
            let key: T = project_to_singleton_key(cols, row_ref);
            let key_size = key.key_size_in_bytes();
            this.insert(key, row_ref.pointer())
                .map_err(|ptr| *ptr)
                .map(|_| key_size)
        }
        fn direct_insert_at_type<T: ReadColumn>(
            this: &mut UniqueDirectIndex,
            cols: &ColList,
            row_ref: RowRef<'_>,
            to_usize: impl FnOnce(T) -> usize,
        ) -> Result<usize, RowPointer> {
            let key: T = project_to_singleton_key(cols, row_ref);
            let key = to_usize(key);
            let key_size = key.key_size_in_bytes();
            this.insert(key, row_ref.pointer()).map(|_| key_size)
        }
        fn direct_u8_insert_at_type<T: ReadColumn>(
            this: &mut UniqueDirectFixedCapIndex,
            cols: &ColList,
            row_ref: RowRef<'_>,
            to_u8: impl FnOnce(T) -> usize,
        ) -> Result<usize, RowPointer> {
            let key: T = project_to_singleton_key(cols, row_ref);
            let key = to_u8(key);
            let key_size = key.key_size_in_bytes();
            this.insert(key, row_ref.pointer()).map(|_| key_size)
        }
        match self {
            Self::BtreeBool(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeU8(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeSumTag(idx) => {
                let SumTag(key) = project_to_singleton_key(cols, row_ref);
                let key_size = key.key_size_in_bytes();
                idx.insert(key, row_ref.pointer());
                Ok(key_size)
            }
            Self::BtreeI8(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeU16(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeI16(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeU32(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeI32(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeU64(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeI64(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeU128(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeI128(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeU256(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeI256(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeString(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::BtreeAV(this) => {
                // SAFETY: Caller promised that any `col` in `cols` is in-bounds of `row_ref`'s layout.
                let key = unsafe { row_ref.project_unchecked(cols) };
                let key_size = key.key_size_in_bytes();
                this.insert(key, row_ref.pointer());
                Ok(key_size)
            }
            Self::UniqueBtreeBool(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeU8(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeSumTag(idx) => {
                let SumTag(key) = project_to_singleton_key(cols, row_ref);
                let key_size = key.key_size_in_bytes();
                idx.insert(key, row_ref.pointer()).map_err(|ptr| *ptr).map(|_| key_size)
            }
            Self::UniqueBtreeI8(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeU16(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeI16(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeU32(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeI32(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeU64(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeI64(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeU128(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeI128(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeU256(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeI256(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeString(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueBtreeAV(this) => {
                // SAFETY: Caller promised that any `col` in `cols` is in-bounds of `row_ref`'s layout.
                let key = unsafe { row_ref.project_unchecked(cols) };
                let key_size = key.key_size_in_bytes();
                this.insert(key, row_ref.pointer())
                    .map_err(|ptr| *ptr)
                    .map(|_| key_size)
            }
            Self::UniqueDirectSumTag(idx) => direct_u8_insert_at_type(idx, cols, row_ref, |SumTag(tag)| tag as usize),
            Self::UniqueDirectU8(idx) => direct_insert_at_type(idx, cols, row_ref, |k: u8| k as usize),
            Self::UniqueDirectU16(idx) => direct_insert_at_type(idx, cols, row_ref, |k: u16| k as usize),
            Self::UniqueDirectU32(idx) => direct_insert_at_type(idx, cols, row_ref, |k: u32| k as usize),
            Self::UniqueDirectU64(idx) => direct_insert_at_type(idx, cols, row_ref, |k: u64| k as usize),
        }
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
    /// If the row was present and has been deleted, returns `Ok(Some(key_size_in_bytes))`,
    /// where `key_size_in_bytes` is the size of the key.
    /// [`TableIndex::delete`] will use this
    /// to update the counter for [`TableIndex::num_key_bytes`].
    /// We want to store said counter outside of the [`TypedIndex`] enum,
    /// but we can only compute the size using type info within the [`TypedIndex`],
    /// so we have to return the size across this boundary.
    // TODO(centril): make this unsafe and use unchecked conversions.
    fn delete(&mut self, cols: &ColList, row_ref: RowRef<'_>) -> Result<Option<usize>, InvalidFieldError> {
        fn mm_delete_at_type<T: Ord + ReadColumn + KeySize>(
            this: &mut BtreeIndex<T>,
            cols: &ColList,
            row_ref: RowRef<'_>,
        ) -> Result<Option<usize>, InvalidFieldError> {
            let col_pos = cols.as_singleton().unwrap();
            let key: T = row_ref.read_col(col_pos).map_err(|_| col_pos)?;
            let key_size = key.key_size_in_bytes();
            Ok(this.delete(&key, &row_ref.pointer()).then_some(key_size))
        }
        fn um_delete_at_type<T: Ord + ReadColumn + KeySize>(
            this: &mut BtreeUniqueIndex<T>,
            cols: &ColList,
            row_ref: RowRef<'_>,
        ) -> Result<Option<usize>, InvalidFieldError> {
            let col_pos = cols.as_singleton().unwrap();
            let key: T = row_ref.read_col(col_pos).map_err(|_| col_pos)?;
            let key_size = key.key_size_in_bytes();
            Ok(this.delete(&key).then_some(key_size))
        }
        fn direct_delete_at_type<T: ReadColumn>(
            this: &mut UniqueDirectIndex,
            cols: &ColList,
            row_ref: RowRef<'_>,
            to_usize: impl FnOnce(T) -> usize,
        ) -> Result<Option<usize>, InvalidFieldError> {
            let col_pos = cols.as_singleton().unwrap();
            let key: T = row_ref.read_col(col_pos).map_err(|_| col_pos)?;
            let key = to_usize(key);
            let key_size = key.key_size_in_bytes();
            Ok(this.delete(key).then_some(key_size))
        }
        fn direct_u8_delete_at_type<T: ReadColumn>(
            this: &mut UniqueDirectFixedCapIndex,
            cols: &ColList,
            row_ref: RowRef<'_>,
            to_u8: impl FnOnce(T) -> usize,
        ) -> Result<Option<usize>, InvalidFieldError> {
            let col_pos = cols.as_singleton().unwrap();
            let key: T = row_ref.read_col(col_pos).map_err(|_| col_pos)?;
            let key = to_u8(key);
            let key_size = key.key_size_in_bytes();
            Ok(this.delete(key).then_some(key_size))
        }

        match self {
            Self::BtreeBool(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeU8(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeSumTag(this) => {
                let col_pos = cols.as_singleton().unwrap();
                let SumTag(key) = row_ref.read_col(col_pos).map_err(|_| col_pos)?;
                let key_size = key.key_size_in_bytes();
                Ok(this.delete(&key, &row_ref.pointer()).then_some(key_size))
            }
            Self::BtreeI8(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeU16(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeI16(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeU32(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeI32(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeU64(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeI64(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeU128(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeI128(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeU256(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeI256(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeString(this) => mm_delete_at_type(this, cols, row_ref),
            Self::BtreeAV(this) => {
                let key = row_ref.project(cols)?;
                let key_size = key.key_size_in_bytes();
                Ok(this.delete(&key, &row_ref.pointer()).then_some(key_size))
            }
            Self::UniqueBtreeBool(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeU8(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeSumTag(this) => {
                let col_pos = cols.as_singleton().unwrap();
                let SumTag(key) = row_ref.read_col(col_pos).map_err(|_| col_pos)?;
                let key_size = key.key_size_in_bytes();
                Ok(this.delete(&key).then_some(key_size))
            }
            Self::UniqueBtreeI8(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeU16(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeI16(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeU32(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeI32(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeU64(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeI64(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeU128(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeI128(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeU256(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeI256(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeString(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueBtreeAV(this) => {
                let key = row_ref.project(cols)?;
                let key_size = key.key_size_in_bytes();
                Ok(this.delete(&key).then_some(key_size))
            }
            Self::UniqueDirectSumTag(this) => direct_u8_delete_at_type(this, cols, row_ref, |SumTag(k)| k as usize),
            Self::UniqueDirectU8(this) => direct_delete_at_type(this, cols, row_ref, |k: u8| k as usize),
            Self::UniqueDirectU16(this) => direct_delete_at_type(this, cols, row_ref, |k: u16| k as usize),
            Self::UniqueDirectU32(this) => direct_delete_at_type(this, cols, row_ref, |k: u32| k as usize),
            Self::UniqueDirectU64(this) => direct_delete_at_type(this, cols, row_ref, |k: u64| k as usize),
        }
    }

    fn seek_point(&self, key: &AlgebraicValue) -> TypedIndexPointIter<'_> {
        fn mm_iter_at_type<'a, T: Ord>(
            this: &'a BtreeIndex<T>,
            key: &AlgebraicValue,
            av_as_t: impl Fn(&AlgebraicValue) -> Option<&T>,
        ) -> BtreeIndexPointIter<'a> {
            this.values_in_point(av_as_t(key).expect("key does not conform to key type of index"))
        }
        fn um_iter_at_type<'a, T: Ord>(
            this: &'a BtreeUniqueIndex<T>,
            key: &AlgebraicValue,
            av_as_t: impl Fn(&AlgebraicValue) -> Option<&T>,
        ) -> BtreeUniqueIndexPointIter<'a> {
            this.values_in_point(av_as_t(key).expect("key does not conform to key type of index"))
        }
        fn direct_iter_at_type<T>(
            this: &UniqueDirectIndex,
            key: &AlgebraicValue,
            av_as_t: impl Fn(&AlgebraicValue) -> Option<&T>,
            to_usize: impl Copy + FnOnce(&T) -> usize,
        ) -> UniqueDirectIndexPointIter {
            let av_as_t = |v| av_as_t(v).expect("key does not conform to key type of index");
            this.seek_point(to_usize(av_as_t(key)))
        }

        use TypedIndex::*;
        use TypedIndexPointIter::*;
        match self {
            BtreeBool(this) => BTree(mm_iter_at_type(this, key, AlgebraicValue::as_bool)),
            BtreeU8(this) => BTree(mm_iter_at_type(this, key, AlgebraicValue::as_u8)),
            BtreeSumTag(this) => BTree(mm_iter_at_type(this, key, as_tag)),
            BtreeI8(this) => BTree(mm_iter_at_type(this, key, AlgebraicValue::as_i8)),
            BtreeU16(this) => BTree(mm_iter_at_type(this, key, AlgebraicValue::as_u16)),
            BtreeI16(this) => BTree(mm_iter_at_type(this, key, AlgebraicValue::as_i16)),
            BtreeU32(this) => BTree(mm_iter_at_type(this, key, AlgebraicValue::as_u32)),
            BtreeI32(this) => BTree(mm_iter_at_type(this, key, AlgebraicValue::as_i32)),
            BtreeU64(this) => BTree(mm_iter_at_type(this, key, AlgebraicValue::as_u64)),
            BtreeI64(this) => BTree(mm_iter_at_type(this, key, AlgebraicValue::as_i64)),
            BtreeU128(this) => BTree(mm_iter_at_type(this, key, AlgebraicValue::as_u128)),
            BtreeI128(this) => BTree(mm_iter_at_type(this, key, AlgebraicValue::as_i128)),
            BtreeU256(this) => BTree(mm_iter_at_type(this, key, |av| av.as_u256().map(|x| &**x))),
            BtreeI256(this) => BTree(mm_iter_at_type(this, key, |av| av.as_i256().map(|x| &**x))),
            BtreeString(this) => BTree(mm_iter_at_type(this, key, AlgebraicValue::as_string)),
            BtreeAV(this) => BTree(this.values_in_point(key)),

            UniqueBtreeBool(this) => UniqueBTree(um_iter_at_type(this, key, AlgebraicValue::as_bool)),
            UniqueBtreeU8(this) => UniqueBTree(um_iter_at_type(this, key, AlgebraicValue::as_u8)),
            UniqueBtreeSumTag(this) => UniqueBTree(um_iter_at_type(this, key, as_tag)),
            UniqueBtreeI8(this) => UniqueBTree(um_iter_at_type(this, key, AlgebraicValue::as_i8)),
            UniqueBtreeU16(this) => UniqueBTree(um_iter_at_type(this, key, AlgebraicValue::as_u16)),
            UniqueBtreeI16(this) => UniqueBTree(um_iter_at_type(this, key, AlgebraicValue::as_i16)),
            UniqueBtreeU32(this) => UniqueBTree(um_iter_at_type(this, key, AlgebraicValue::as_u32)),
            UniqueBtreeI32(this) => UniqueBTree(um_iter_at_type(this, key, AlgebraicValue::as_i32)),
            UniqueBtreeU64(this) => UniqueBTree(um_iter_at_type(this, key, AlgebraicValue::as_u64)),
            UniqueBtreeI64(this) => UniqueBTree(um_iter_at_type(this, key, AlgebraicValue::as_i64)),
            UniqueBtreeU128(this) => UniqueBTree(um_iter_at_type(this, key, AlgebraicValue::as_u128)),
            UniqueBtreeI128(this) => UniqueBTree(um_iter_at_type(this, key, AlgebraicValue::as_i128)),
            UniqueBtreeU256(this) => UniqueBTree(um_iter_at_type(this, key, |av| av.as_u256().map(|x| &**x))),
            UniqueBtreeI256(this) => UniqueBTree(um_iter_at_type(this, key, |av| av.as_i256().map(|x| &**x))),
            UniqueBtreeString(this) => UniqueBTree(um_iter_at_type(this, key, AlgebraicValue::as_string)),
            UniqueBtreeAV(this) => UniqueBTree(this.values_in_point(key)),

            UniqueDirectSumTag(this) => {
                let key = as_tag(key).expect("key does not conform to key type of index");
                UniqueDirect(this.seek_point(*key as usize))
            }
            UniqueDirectU8(this) => {
                UniqueDirect(direct_iter_at_type(this, key, AlgebraicValue::as_u8, |k| *k as usize))
            }
            UniqueDirectU16(this) => {
                UniqueDirect(direct_iter_at_type(this, key, AlgebraicValue::as_u16, |k| *k as usize))
            }
            UniqueDirectU32(this) => {
                UniqueDirect(direct_iter_at_type(this, key, AlgebraicValue::as_u32, |k| *k as usize))
            }
            UniqueDirectU64(this) => {
                UniqueDirect(direct_iter_at_type(this, key, AlgebraicValue::as_u64, |k| *k as usize))
            }
        }
    }

    fn seek_range(&self, range: &impl RangeBounds<AlgebraicValue>) -> TypedIndexRangeIter<'_> {
        fn mm_iter_at_type<'a, T: Ord>(
            this: &'a BtreeIndex<T>,
            range: &impl RangeBounds<AlgebraicValue>,
            av_as_t: impl Fn(&AlgebraicValue) -> Option<&T>,
        ) -> BtreeIndexRangeIter<'a, T> {
            let av_as_t = |v| av_as_t(v).expect("bound does not conform to key type of index");
            let start = range.start_bound().map(av_as_t);
            let end = range.end_bound().map(av_as_t);
            this.values_in_range(&(start, end))
        }
        fn um_iter_at_type<'a, T: Ord>(
            this: &'a BtreeUniqueIndex<T>,
            range: &impl RangeBounds<AlgebraicValue>,
            av_as_t: impl Fn(&AlgebraicValue) -> Option<&T>,
        ) -> BtreeUniqueIndexRangeIter<'a, T> {
            let av_as_t = |v| av_as_t(v).expect("bound does not conform to key type of index");
            let start = range.start_bound().map(av_as_t);
            let end = range.end_bound().map(av_as_t);
            this.values_in_range(&(start, end))
        }
        fn direct_iter_at_type<'a, T>(
            this: &'a UniqueDirectIndex,
            range: &impl RangeBounds<AlgebraicValue>,
            av_as_t: impl Fn(&AlgebraicValue) -> Option<&T>,
            to_usize: impl Copy + FnOnce(&T) -> usize,
        ) -> UniqueDirectIndexRangeIter<'a> {
            let av_as_t = |v| av_as_t(v).expect("bound does not conform to key type of index");
            let start = range.start_bound().map(av_as_t).map(to_usize);
            let end = range.end_bound().map(av_as_t).map(to_usize);
            this.seek_range(&(start, end))
        }

        use TypedIndexRangeIter::*;
        match self {
            Self::BtreeBool(this) => BtreeBool(mm_iter_at_type(this, range, AlgebraicValue::as_bool)),
            Self::BtreeU8(this) => BtreeU8(mm_iter_at_type(this, range, AlgebraicValue::as_u8)),
            Self::BtreeSumTag(this) => BtreeU8(mm_iter_at_type(this, range, as_tag)),
            Self::BtreeI8(this) => BtreeI8(mm_iter_at_type(this, range, AlgebraicValue::as_i8)),
            Self::BtreeU16(this) => BtreeU16(mm_iter_at_type(this, range, AlgebraicValue::as_u16)),
            Self::BtreeI16(this) => BtreeI16(mm_iter_at_type(this, range, AlgebraicValue::as_i16)),
            Self::BtreeU32(this) => BtreeU32(mm_iter_at_type(this, range, AlgebraicValue::as_u32)),
            Self::BtreeI32(this) => BtreeI32(mm_iter_at_type(this, range, AlgebraicValue::as_i32)),
            Self::BtreeU64(this) => BtreeU64(mm_iter_at_type(this, range, AlgebraicValue::as_u64)),
            Self::BtreeI64(this) => BtreeI64(mm_iter_at_type(this, range, AlgebraicValue::as_i64)),
            Self::BtreeU128(this) => BtreeU128(mm_iter_at_type(this, range, AlgebraicValue::as_u128)),
            Self::BtreeI128(this) => BtreeI128(mm_iter_at_type(this, range, AlgebraicValue::as_i128)),
            Self::BtreeU256(this) => BtreeU256(mm_iter_at_type(this, range, |av| av.as_u256().map(|x| &**x))),
            Self::BtreeI256(this) => BtreeI256(mm_iter_at_type(this, range, |av| av.as_i256().map(|x| &**x))),
            Self::BtreeString(this) => BtreeString(mm_iter_at_type(this, range, AlgebraicValue::as_string)),
            Self::BtreeAV(this) => BtreeAV(this.values_in_range(range)),

            Self::UniqueBtreeBool(this) => UniqueBtreeBool(um_iter_at_type(this, range, AlgebraicValue::as_bool)),
            Self::UniqueBtreeU8(this) => UniqueBtreeU8(um_iter_at_type(this, range, AlgebraicValue::as_u8)),
            Self::UniqueBtreeSumTag(this) => UniqueBtreeU8(um_iter_at_type(this, range, as_tag)),
            Self::UniqueBtreeI8(this) => UniqueBtreeI8(um_iter_at_type(this, range, AlgebraicValue::as_i8)),
            Self::UniqueBtreeU16(this) => UniqueBtreeU16(um_iter_at_type(this, range, AlgebraicValue::as_u16)),
            Self::UniqueBtreeI16(this) => UniqueBtreeI16(um_iter_at_type(this, range, AlgebraicValue::as_i16)),
            Self::UniqueBtreeU32(this) => UniqueBtreeU32(um_iter_at_type(this, range, AlgebraicValue::as_u32)),
            Self::UniqueBtreeI32(this) => UniqueBtreeI32(um_iter_at_type(this, range, AlgebraicValue::as_i32)),
            Self::UniqueBtreeU64(this) => UniqueBtreeU64(um_iter_at_type(this, range, AlgebraicValue::as_u64)),
            Self::UniqueBtreeI64(this) => UniqueBtreeI64(um_iter_at_type(this, range, AlgebraicValue::as_i64)),
            Self::UniqueBtreeU128(this) => UniqueBtreeU128(um_iter_at_type(this, range, AlgebraicValue::as_u128)),
            Self::UniqueBtreeI128(this) => UniqueBtreeI128(um_iter_at_type(this, range, AlgebraicValue::as_i128)),
            Self::UniqueBtreeU256(this) => {
                UniqueBtreeU256(um_iter_at_type(this, range, |av| av.as_u256().map(|x| &**x)))
            }
            Self::UniqueBtreeI256(this) => {
                UniqueBtreeI256(um_iter_at_type(this, range, |av| av.as_i256().map(|x| &**x)))
            }
            Self::UniqueBtreeString(this) => UniqueBtreeString(um_iter_at_type(this, range, AlgebraicValue::as_string)),
            Self::UniqueBtreeAV(this) => UniqueBtreeAV(this.values_in_range(range)),

            Self::UniqueDirectSumTag(this) => {
                let av_as_t = |v| as_tag(v).copied().expect("bound does not conform to key type of index") as usize;
                let start = range.start_bound().map(av_as_t);
                let end = range.end_bound().map(av_as_t);
                let iter = this.seek_range(&(start, end));
                UniqueDirectU8(iter)
            }
            Self::UniqueDirectU8(this) => {
                UniqueDirect(direct_iter_at_type(this, range, AlgebraicValue::as_u8, |k| *k as usize))
            }
            Self::UniqueDirectU16(this) => {
                UniqueDirect(direct_iter_at_type(this, range, AlgebraicValue::as_u16, |k| {
                    *k as usize
                }))
            }
            Self::UniqueDirectU32(this) => {
                UniqueDirect(direct_iter_at_type(this, range, AlgebraicValue::as_u32, |k| {
                    *k as usize
                }))
            }
            Self::UniqueDirectU64(this) => {
                UniqueDirect(direct_iter_at_type(this, range, AlgebraicValue::as_u64, |k| {
                    *k as usize
                }))
            }
        }
    }

    fn clear(&mut self) {
        match self {
            Self::BtreeBool(this) => this.clear(),
            Self::BtreeU8(this) | Self::BtreeSumTag(this) => this.clear(),
            Self::BtreeI8(this) => this.clear(),
            Self::BtreeU16(this) => this.clear(),
            Self::BtreeI16(this) => this.clear(),
            Self::BtreeU32(this) => this.clear(),
            Self::BtreeI32(this) => this.clear(),
            Self::BtreeU64(this) => this.clear(),
            Self::BtreeI64(this) => this.clear(),
            Self::BtreeU128(this) => this.clear(),
            Self::BtreeI128(this) => this.clear(),
            Self::BtreeU256(this) => this.clear(),
            Self::BtreeI256(this) => this.clear(),
            Self::BtreeString(this) => this.clear(),
            Self::BtreeAV(this) => this.clear(),

            Self::UniqueBtreeBool(this) => this.clear(),
            Self::UniqueBtreeU8(this) | Self::UniqueBtreeSumTag(this) => this.clear(),
            Self::UniqueBtreeI8(this) => this.clear(),
            Self::UniqueBtreeU16(this) => this.clear(),
            Self::UniqueBtreeI16(this) => this.clear(),
            Self::UniqueBtreeU32(this) => this.clear(),
            Self::UniqueBtreeI32(this) => this.clear(),
            Self::UniqueBtreeU64(this) => this.clear(),
            Self::UniqueBtreeI64(this) => this.clear(),
            Self::UniqueBtreeU128(this) => this.clear(),
            Self::UniqueBtreeI128(this) => this.clear(),
            Self::UniqueBtreeU256(this) => this.clear(),
            Self::UniqueBtreeI256(this) => this.clear(),
            Self::UniqueBtreeString(this) => this.clear(),
            Self::UniqueBtreeAV(this) => this.clear(),

            Self::UniqueDirectSumTag(this) => this.clear(),
            Self::UniqueDirectU8(this)
            | Self::UniqueDirectU16(this)
            | Self::UniqueDirectU32(this)
            | Self::UniqueDirectU64(this) => this.clear(),
        }
    }

    #[allow(unused)] // used only by tests
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[allow(unused)] // used only by tests
    fn len(&self) -> usize {
        match self {
            Self::BtreeBool(this) => this.len(),
            Self::BtreeU8(this) | Self::BtreeSumTag(this) => this.len(),
            Self::BtreeI8(this) => this.len(),
            Self::BtreeU16(this) => this.len(),
            Self::BtreeI16(this) => this.len(),
            Self::BtreeU32(this) => this.len(),
            Self::BtreeI32(this) => this.len(),
            Self::BtreeU64(this) => this.len(),
            Self::BtreeI64(this) => this.len(),
            Self::BtreeU128(this) => this.len(),
            Self::BtreeI128(this) => this.len(),
            Self::BtreeU256(this) => this.len(),
            Self::BtreeI256(this) => this.len(),
            Self::BtreeString(this) => this.len(),
            Self::BtreeAV(this) => this.len(),

            Self::UniqueBtreeBool(this) => this.len(),
            Self::UniqueBtreeU8(this) | Self::UniqueBtreeSumTag(this) => this.len(),
            Self::UniqueBtreeI8(this) => this.len(),
            Self::UniqueBtreeU16(this) => this.len(),
            Self::UniqueBtreeI16(this) => this.len(),
            Self::UniqueBtreeU32(this) => this.len(),
            Self::UniqueBtreeI32(this) => this.len(),
            Self::UniqueBtreeU64(this) => this.len(),
            Self::UniqueBtreeI64(this) => this.len(),
            Self::UniqueBtreeU128(this) => this.len(),
            Self::UniqueBtreeI128(this) => this.len(),
            Self::UniqueBtreeU256(this) => this.len(),
            Self::UniqueBtreeI256(this) => this.len(),
            Self::UniqueBtreeString(this) => this.len(),
            Self::UniqueBtreeAV(this) => this.len(),

            Self::UniqueDirectSumTag(this) => this.len(),
            Self::UniqueDirectU8(this)
            | Self::UniqueDirectU16(this)
            | Self::UniqueDirectU32(this)
            | Self::UniqueDirectU64(this) => this.len(),
        }
    }

    fn num_keys(&self) -> usize {
        match self {
            Self::BtreeBool(this) => this.num_keys(),
            Self::BtreeU8(this) | Self::BtreeSumTag(this) => this.num_keys(),
            Self::BtreeI8(this) => this.num_keys(),
            Self::BtreeU16(this) => this.num_keys(),
            Self::BtreeI16(this) => this.num_keys(),
            Self::BtreeU32(this) => this.num_keys(),
            Self::BtreeI32(this) => this.num_keys(),
            Self::BtreeU64(this) => this.num_keys(),
            Self::BtreeI64(this) => this.num_keys(),
            Self::BtreeU128(this) => this.num_keys(),
            Self::BtreeI128(this) => this.num_keys(),
            Self::BtreeU256(this) => this.num_keys(),
            Self::BtreeI256(this) => this.num_keys(),
            Self::BtreeString(this) => this.num_keys(),
            Self::BtreeAV(this) => this.num_keys(),

            Self::UniqueBtreeBool(this) => this.num_keys(),
            Self::UniqueBtreeU8(this) | Self::UniqueBtreeSumTag(this) => this.num_keys(),
            Self::UniqueBtreeI8(this) => this.num_keys(),
            Self::UniqueBtreeU16(this) => this.num_keys(),
            Self::UniqueBtreeI16(this) => this.num_keys(),
            Self::UniqueBtreeU32(this) => this.num_keys(),
            Self::UniqueBtreeI32(this) => this.num_keys(),
            Self::UniqueBtreeU64(this) => this.num_keys(),
            Self::UniqueBtreeI64(this) => this.num_keys(),
            Self::UniqueBtreeU128(this) => this.num_keys(),
            Self::UniqueBtreeI128(this) => this.num_keys(),
            Self::UniqueBtreeU256(this) => this.num_keys(),
            Self::UniqueBtreeI256(this) => this.num_keys(),
            Self::UniqueBtreeString(this) => this.num_keys(),
            Self::UniqueBtreeAV(this) => this.num_keys(),

            Self::UniqueDirectSumTag(this) => this.num_keys(),
            Self::UniqueDirectU8(this)
            | Self::UniqueDirectU16(this)
            | Self::UniqueDirectU32(this)
            | Self::UniqueDirectU64(this) => this.num_keys(),
        }
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

    /// The number of rows in this index.
    ///
    /// Memoized counter for [`Self::num_rows`].
    num_rows: u64,

    /// The number of key bytes in this index.
    ///
    /// Memoized counter for [`Self::num_key_bytes`].
    /// See that method for more detailed documentation.
    num_key_bytes: u64,

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
            num_rows,
            num_key_bytes,
            indexed_columns,
        } = self;
        idx.heap_usage()
            + key_type.heap_usage()
            + num_rows.heap_usage()
            + num_key_bytes.heap_usage()
            + indexed_columns.heap_usage()
    }
}

static_assert_size!(TableIndex, 88);

impl TableIndex {
    /// Returns a new possibly unique index, with `index_id` for a choice of indexing algorithm.
    pub fn new(
        row_type: &ProductType,
        index_algo: &IndexAlgorithm,
        is_unique: bool,
    ) -> Result<Self, InvalidFieldError> {
        let indexed_columns = index_algo.columns().to_owned();
        let key_type = row_type.project(&indexed_columns)?;
        let typed_index = TypedIndex::new(&key_type, index_algo, is_unique);
        Ok(Self {
            idx: typed_index,
            key_type,
            num_rows: 0,
            num_key_bytes: 0,
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
            num_rows: 0,
            num_key_bytes: 0,
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
        let res = unsafe { self.idx.insert(&self.indexed_columns, row_ref) };
        match res {
            Ok(key_size) => {
                // No existing row; the new row was inserted.
                // Update the `num_rows` and `num_key_bytes` counters
                // to account for the new insertion.
                self.num_rows += 1;
                self.num_key_bytes += key_size as u64;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Deletes `row_ref` with its indexed value `row_ref.project(&self.indexed_columns)` from this index.
    ///
    /// Returns whether `ptr` was present.
    pub fn delete(&mut self, row_ref: RowRef<'_>) -> Result<bool, InvalidFieldError> {
        if let Some(size_in_bytes) = self.idx.delete(&self.indexed_columns, row_ref)? {
            // Was present, and deleted: update the `num_rows` and `num_key_bytes` counters.
            self.num_rows -= 1;
            self.num_key_bytes -= size_in_bytes as u64;
            Ok(true)
        } else {
            // Was not present: don't update counters.
            Ok(false)
        }
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

    /// Returns an iterator over the [TableIndex] that yields all the `RowPointer`s
    /// that fall within the specified `range`.
    pub fn seek_range(&self, range: &impl RangeBounds<AlgebraicValue>) -> TableIndexRangeIter<'_> {
        TableIndexRangeIter {
            iter: self.idx.seek_range(range),
        }
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
            | (BtreeString(_), BtreeString(_))
            | (BtreeAV(_), BtreeAV(_)) => Ok(()),
            // For unique indices, we'll need to see if everything in `other` can be added to `idx`.
            (UniqueBtreeBool(idx), UniqueBtreeBool(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeU8(idx), UniqueBtreeU8(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeSumTag(idx), UniqueBtreeSumTag(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeI8(idx), UniqueBtreeI8(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeU16(idx), UniqueBtreeU16(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeI16(idx), UniqueBtreeI16(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeU32(idx), UniqueBtreeU32(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeI32(idx), UniqueBtreeI32(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeU64(idx), UniqueBtreeU64(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeI64(idx), UniqueBtreeI64(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeU128(idx), UniqueBtreeU128(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeI128(idx), UniqueBtreeI128(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeU256(idx), UniqueBtreeU256(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeI256(idx), UniqueBtreeI256(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeString(idx), UniqueBtreeString(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
            (UniqueBtreeAV(idx), UniqueBtreeAV(other)) => idx.can_merge(other, ignore).map_err(|ptr| *ptr),
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
        self.num_key_bytes = 0;
        self.num_rows = 0;
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
        self.num_rows
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
        self.num_key_bytes
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::page_pool::PagePool;
    use crate::{blob_store::HashMapBlobStore, table::test::table};
    use core::ops::Bound::*;
    use proptest::prelude::*;
    use proptest::{collection::vec, test_runner::TestCaseResult};
    use spacetimedb_data_structures::map::HashMap;
    use spacetimedb_primitives::ColId;
    use spacetimedb_sats::{
        product,
        proptest::{generate_product_value, generate_row_type},
        AlgebraicType, ProductType, ProductValue,
    };
    use spacetimedb_schema::def::BTreeAlgorithm;

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

    fn new_index(row_type: &ProductType, cols: &ColList, is_unique: bool) -> TableIndex {
        let algo = BTreeAlgorithm { columns: cols.clone() }.into();
        TableIndex::new(row_type, &algo, is_unique).unwrap()
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

    proptest! {
        #![proptest_config(ProptestConfig { max_shrink_iters: 0x10000000, ..Default::default() })]
        #[test]
        fn remove_nonexistent_noop(((ty, cols, pv), is_unique) in (gen_row_and_cols(), any::<bool>())) {
            let mut index = new_index(&ty, &cols, is_unique);
            let mut table = table(ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let row_ref = table.insert(&pool, &mut blob_store, &pv).unwrap().1;
            prop_assert_eq!(index.delete(row_ref).unwrap(), false);
            prop_assert!(index.idx.is_empty());
        }

        #[test]
        fn insert_delete_noop(((ty, cols, pv), is_unique) in (gen_row_and_cols(), any::<bool>())) {
            let mut index = new_index(&ty, &cols, is_unique);
            let mut table = table(ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let row_ref = table.insert(&pool, &mut blob_store, &pv).unwrap().1;
            let value = get_fields(&cols, &pv);

            prop_assert_eq!(index.idx.len(), 0);
            prop_assert_eq!(index.contains_any(&value), false);

            prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Ok(()));
            prop_assert_eq!(index.idx.len(), 1);
            prop_assert_eq!(index.contains_any(&value), true);

            prop_assert_eq!(index.delete(row_ref).unwrap(), true);
            prop_assert_eq!(index.idx.len(), 0);
            prop_assert_eq!(index.contains_any(&value), false);
        }

        #[test]
        fn insert_again_violates_unique_constraint((ty, cols, pv) in gen_row_and_cols()) {
            let mut index = new_index(&ty, &cols, true);
            let mut table = table(ty);
            let pool = PagePool::new_for_test();
            let mut blob_store = HashMapBlobStore::default();
            let row_ref = table.insert(&pool, &mut blob_store, &pv).unwrap().1;
            let value = get_fields(&cols, &pv);

            // Nothing in the index yet.
            prop_assert_eq!(index.idx.len(), 0);
            prop_assert_eq!(violates_unique_constraint(&index, &cols, &pv), false);
            prop_assert_eq!(
                get_rows_that_violate_unique_constraint(&index, &value).unwrap().collect::<Vec<_>>(),
                []
            );

            // Insert.
            // SAFETY: `row_ref` has the same type as was passed in when constructing `index`.
            prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Ok(()));

            // Inserting again would be a problem.
            prop_assert_eq!(index.idx.len(), 1);
            prop_assert_eq!(violates_unique_constraint(&index, &cols, &pv), true);
            prop_assert_eq!(
                get_rows_that_violate_unique_constraint(&index, &value).unwrap().collect::<Vec<_>>(),
                [row_ref.pointer()]
            );
            // SAFETY: `row_ref` has the same type as was passed in when constructing `index`.
            prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Err(row_ref.pointer()));
        }

        #[test]
        fn seek_various_ranges(needle in 1..u64::MAX) {
            use AlgebraicValue::U64 as V;

            let cols = 0.into();
            let ty = ProductType::from_iter([AlgebraicType::U64]);
            let mut index = new_index(&ty, &cols, true);
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

            fn test_seek(index: &TableIndex, val_to_ptr: &HashMap<u64, RowPointer>, range: impl RangeBounds<AlgebraicValue>, expect: impl IntoIterator<Item = u64>) -> TestCaseResult {
                let mut ptrs_in_index = index.seek_range(&range).collect::<Vec<_>>();
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
    }
}
