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
// TODO(centril): the `BTreeIndex` naming makes no sense now.
// Rename to `TableIndex`.
use super::indexes::RowPointer;
use super::table::RowRef;
use crate::{read_column::ReadColumn, static_assert_size, MemoryUsage};
use core::ops::RangeBounds;
use spacetimedb_primitives::ColList;
use spacetimedb_sats::{
    algebraic_value::Packed, i256, product_value::InvalidFieldError, u256, AlgebraicType, AlgebraicValue, ProductType,
};

mod key_size;
mod multimap;
pub mod unique_direct_index;
pub mod uniquemap;

pub use key_size::KeySize;
use spacetimedb_schema::def::IndexAlgorithm;
use unique_direct_index::{UniqueDirectIndex, UniqueDirectIndexRangeIter};

type BtreeIndex<K> = multimap::MultiMap<K, RowPointer>;
type BtreeIndexRangeIter<'a, K> = multimap::MultiMapRangeIter<'a, K, RowPointer>;
type BtreeUniqueIndex<K> = uniquemap::UniqueMap<K, RowPointer>;
type BtreeUniqueIndexRangeIter<'a, K> = uniquemap::UniqueMapRangeIter<'a, K, RowPointer>;

/// A ranged iterator over a [`TypedIndex`], with a specialized key type.
///
/// See module docs for info about specialization.
enum TypedIndexRangeIter<'a> {
    // All the non-unique index iterators.
    Bool(BtreeIndexRangeIter<'a, bool>),
    U8(BtreeIndexRangeIter<'a, u8>),
    I8(BtreeIndexRangeIter<'a, i8>),
    U16(BtreeIndexRangeIter<'a, u16>),
    I16(BtreeIndexRangeIter<'a, i16>),
    U32(BtreeIndexRangeIter<'a, u32>),
    I32(BtreeIndexRangeIter<'a, i32>),
    U64(BtreeIndexRangeIter<'a, u64>),
    I64(BtreeIndexRangeIter<'a, i64>),
    U128(BtreeIndexRangeIter<'a, Packed<u128>>),
    I128(BtreeIndexRangeIter<'a, Packed<i128>>),
    U256(BtreeIndexRangeIter<'a, u256>),
    I256(BtreeIndexRangeIter<'a, i256>),
    String(BtreeIndexRangeIter<'a, Box<str>>),
    AV(BtreeIndexRangeIter<'a, AlgebraicValue>),

    // All the unique index iterators.
    UniqueBool(BtreeUniqueIndexRangeIter<'a, bool>),
    UniqueU8(BtreeUniqueIndexRangeIter<'a, u8>),
    UniqueI8(BtreeUniqueIndexRangeIter<'a, i8>),
    UniqueU16(BtreeUniqueIndexRangeIter<'a, u16>),
    UniqueI16(BtreeUniqueIndexRangeIter<'a, i16>),
    UniqueU32(BtreeUniqueIndexRangeIter<'a, u32>),
    UniqueI32(BtreeUniqueIndexRangeIter<'a, i32>),
    UniqueU64(BtreeUniqueIndexRangeIter<'a, u64>),
    UniqueI64(BtreeUniqueIndexRangeIter<'a, i64>),
    UniqueU128(BtreeUniqueIndexRangeIter<'a, Packed<u128>>),
    UniqueI128(BtreeUniqueIndexRangeIter<'a, Packed<i128>>),
    UniqueU256(BtreeUniqueIndexRangeIter<'a, u256>),
    UniqueI256(BtreeUniqueIndexRangeIter<'a, i256>),
    UniqueString(BtreeUniqueIndexRangeIter<'a, Box<str>>),
    UniqueAV(BtreeUniqueIndexRangeIter<'a, AlgebraicValue>),

    UniqueDirect(UniqueDirectIndexRangeIter<'a>),
}

impl Iterator for TypedIndexRangeIter<'_> {
    type Item = RowPointer;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Bool(this) => this.next().copied(),
            Self::U8(this) => this.next().copied(),
            Self::I8(this) => this.next().copied(),
            Self::U16(this) => this.next().copied(),
            Self::I16(this) => this.next().copied(),
            Self::U32(this) => this.next().copied(),
            Self::I32(this) => this.next().copied(),
            Self::U64(this) => this.next().copied(),
            Self::I64(this) => this.next().copied(),
            Self::U128(this) => this.next().copied(),
            Self::I128(this) => this.next().copied(),
            Self::U256(this) => this.next().copied(),
            Self::I256(this) => this.next().copied(),
            Self::String(this) => this.next().copied(),
            Self::AV(this) => this.next().copied(),

            Self::UniqueBool(this) => this.next().copied(),
            Self::UniqueU8(this) => this.next().copied(),
            Self::UniqueI8(this) => this.next().copied(),
            Self::UniqueU16(this) => this.next().copied(),
            Self::UniqueI16(this) => this.next().copied(),
            Self::UniqueU32(this) => this.next().copied(),
            Self::UniqueI32(this) => this.next().copied(),
            Self::UniqueU64(this) => this.next().copied(),
            Self::UniqueI64(this) => this.next().copied(),
            Self::UniqueU128(this) => this.next().copied(),
            Self::UniqueI128(this) => this.next().copied(),
            Self::UniqueU256(this) => this.next().copied(),
            Self::UniqueI256(this) => this.next().copied(),
            Self::UniqueString(this) => this.next().copied(),
            Self::UniqueAV(this) => this.next().copied(),

            Self::UniqueDirect(this) => this.next(),
        }
    }
}

/// An iterator over rows matching a certain [`AlgebraicValue`] on the [`BTreeIndex`].
pub struct BTreeIndexRangeIter<'a> {
    /// The iterator seeking for matching values.
    iter: TypedIndexRangeIter<'a>,
}

impl Iterator for BTreeIndexRangeIter<'_> {
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
    Bool(BtreeIndex<bool>),
    U8(BtreeIndex<u8>),
    I8(BtreeIndex<i8>),
    U16(BtreeIndex<u16>),
    I16(BtreeIndex<i16>),
    U32(BtreeIndex<u32>),
    I32(BtreeIndex<i32>),
    U64(BtreeIndex<u64>),
    I64(BtreeIndex<i64>),
    U128(BtreeIndex<Packed<u128>>),
    I128(BtreeIndex<Packed<i128>>),
    U256(BtreeIndex<u256>),
    I256(BtreeIndex<i256>),
    String(BtreeIndex<Box<str>>),
    AV(BtreeIndex<AlgebraicValue>),

    // All the unique btree index types.
    UniqueBool(BtreeUniqueIndex<bool>),
    UniqueU8(BtreeUniqueIndex<u8>),
    UniqueI8(BtreeUniqueIndex<i8>),
    UniqueU16(BtreeUniqueIndex<u16>),
    UniqueI16(BtreeUniqueIndex<i16>),
    UniqueU32(BtreeUniqueIndex<u32>),
    UniqueI32(BtreeUniqueIndex<i32>),
    UniqueU64(BtreeUniqueIndex<u64>),
    UniqueI64(BtreeUniqueIndex<i64>),
    UniqueU128(BtreeUniqueIndex<Packed<u128>>),
    UniqueI128(BtreeUniqueIndex<Packed<i128>>),
    UniqueU256(BtreeUniqueIndex<u256>),
    UniqueI256(BtreeUniqueIndex<i256>),
    UniqueString(BtreeUniqueIndex<Box<str>>),
    UniqueAV(BtreeUniqueIndex<AlgebraicValue>),

    // All the unique direct index types.
    UniqueDirectU8(UniqueDirectIndex),
    UniqueDirectU16(UniqueDirectIndex),
    UniqueDirectU32(UniqueDirectIndex),
    UniqueDirectU64(UniqueDirectIndex),
}

impl MemoryUsage for TypedIndex {
    fn heap_usage(&self) -> usize {
        match self {
            TypedIndex::Bool(this) => this.heap_usage(),
            TypedIndex::U8(this) => this.heap_usage(),
            TypedIndex::I8(this) => this.heap_usage(),
            TypedIndex::U16(this) => this.heap_usage(),
            TypedIndex::I16(this) => this.heap_usage(),
            TypedIndex::U32(this) => this.heap_usage(),
            TypedIndex::I32(this) => this.heap_usage(),
            TypedIndex::U64(this) => this.heap_usage(),
            TypedIndex::I64(this) => this.heap_usage(),
            TypedIndex::U128(this) => this.heap_usage(),
            TypedIndex::I128(this) => this.heap_usage(),
            TypedIndex::U256(this) => this.heap_usage(),
            TypedIndex::I256(this) => this.heap_usage(),
            TypedIndex::String(this) => this.heap_usage(),
            TypedIndex::AV(this) => this.heap_usage(),

            TypedIndex::UniqueBool(this) => this.heap_usage(),
            TypedIndex::UniqueU8(this) => this.heap_usage(),
            TypedIndex::UniqueI8(this) => this.heap_usage(),
            TypedIndex::UniqueU16(this) => this.heap_usage(),
            TypedIndex::UniqueI16(this) => this.heap_usage(),
            TypedIndex::UniqueU32(this) => this.heap_usage(),
            TypedIndex::UniqueI32(this) => this.heap_usage(),
            TypedIndex::UniqueU64(this) => this.heap_usage(),
            TypedIndex::UniqueI64(this) => this.heap_usage(),
            TypedIndex::UniqueU128(this) => this.heap_usage(),
            TypedIndex::UniqueI128(this) => this.heap_usage(),
            TypedIndex::UniqueU256(this) => this.heap_usage(),
            TypedIndex::UniqueI256(this) => this.heap_usage(),
            TypedIndex::UniqueString(this) => this.heap_usage(),
            TypedIndex::UniqueAV(this) => this.heap_usage(),

            TypedIndex::UniqueDirectU8(this)
            | TypedIndex::UniqueDirectU16(this)
            | TypedIndex::UniqueDirectU32(this)
            | TypedIndex::UniqueDirectU64(this) => this.heap_usage(),
        }
    }
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
                _ => unreachable!("unexpected key type {key_type:?} for direct index"),
            };
        }

        // If the index is on a single column of a primitive type,
        // use a homogeneous map with a native key type.
        if is_unique {
            match key_type {
                AlgebraicType::Bool => UniqueBool(<_>::default()),
                AlgebraicType::I8 => UniqueI8(<_>::default()),
                AlgebraicType::U8 => UniqueU8(<_>::default()),
                AlgebraicType::I16 => UniqueI16(<_>::default()),
                AlgebraicType::U16 => UniqueU16(<_>::default()),
                AlgebraicType::I32 => UniqueI32(<_>::default()),
                AlgebraicType::U32 => UniqueU32(<_>::default()),
                AlgebraicType::I64 => UniqueI64(<_>::default()),
                AlgebraicType::U64 => UniqueU64(<_>::default()),
                AlgebraicType::I128 => UniqueI128(<_>::default()),
                AlgebraicType::U128 => UniqueU128(<_>::default()),
                AlgebraicType::I256 => UniqueI256(<_>::default()),
                AlgebraicType::U256 => UniqueU256(<_>::default()),
                AlgebraicType::String => UniqueString(<_>::default()),

                // The index is either multi-column,
                // or we don't care to specialize on the key type,
                // so use a map keyed on `AlgebraicValue`.
                _ => UniqueAV(<_>::default()),
            }
        } else {
            match key_type {
                AlgebraicType::Bool => Bool(<_>::default()),
                AlgebraicType::I8 => I8(<_>::default()),
                AlgebraicType::U8 => U8(<_>::default()),
                AlgebraicType::I16 => I16(<_>::default()),
                AlgebraicType::U16 => U16(<_>::default()),
                AlgebraicType::I32 => I32(<_>::default()),
                AlgebraicType::U32 => U32(<_>::default()),
                AlgebraicType::I64 => I64(<_>::default()),
                AlgebraicType::U64 => U64(<_>::default()),
                AlgebraicType::I128 => I128(<_>::default()),
                AlgebraicType::U128 => U128(<_>::default()),
                AlgebraicType::I256 => I256(<_>::default()),
                AlgebraicType::U256 => U256(<_>::default()),
                AlgebraicType::String => String(<_>::default()),

                // The index is either multi-column,
                // or we don't care to specialize on the key type,
                // so use a map keyed on `AlgebraicValue`.
                _ => AV(<_>::default()),
            }
        }
    }

    /// Clones the structure of this index but not the indexed elements,
    /// so the returned index is empty.
    fn clone_structure(&self) -> Self {
        use TypedIndex::*;
        match self {
            Bool(_) => Bool(<_>::default()),
            U8(_) => U8(<_>::default()),
            I8(_) => I8(<_>::default()),
            U16(_) => U16(<_>::default()),
            I16(_) => I16(<_>::default()),
            U32(_) => U32(<_>::default()),
            I32(_) => I32(<_>::default()),
            U64(_) => U64(<_>::default()),
            I64(_) => I64(<_>::default()),
            U128(_) => U128(<_>::default()),
            I128(_) => I128(<_>::default()),
            U256(_) => U256(<_>::default()),
            I256(_) => I256(<_>::default()),
            String(_) => String(<_>::default()),
            AV(_) => AV(<_>::default()),
            UniqueBool(_) => UniqueBool(<_>::default()),
            UniqueU8(_) => UniqueU8(<_>::default()),
            UniqueI8(_) => UniqueI8(<_>::default()),
            UniqueU16(_) => UniqueU16(<_>::default()),
            UniqueI16(_) => UniqueI16(<_>::default()),
            UniqueU32(_) => UniqueU32(<_>::default()),
            UniqueI32(_) => UniqueI32(<_>::default()),
            UniqueU64(_) => UniqueU64(<_>::default()),
            UniqueI64(_) => UniqueI64(<_>::default()),
            UniqueU128(_) => UniqueU128(<_>::default()),
            UniqueI128(_) => UniqueI128(<_>::default()),
            UniqueU256(_) => UniqueU256(<_>::default()),
            UniqueI256(_) => UniqueI256(<_>::default()),
            UniqueString(_) => UniqueString(<_>::default()),
            UniqueAV(_) => UniqueAV(<_>::default()),
            UniqueDirectU8(_) => UniqueDirectU8(<_>::default()),
            UniqueDirectU16(_) => UniqueDirectU16(<_>::default()),
            UniqueDirectU32(_) => UniqueDirectU32(<_>::default()),
            UniqueDirectU64(_) => UniqueDirectU64(<_>::default()),
        }
    }

    /// Returns whether this is a unique index or not.
    fn is_unique(&self) -> bool {
        use TypedIndex::*;
        match self {
            Bool(_) | U8(_) | I8(_) | U16(_) | I16(_) | U32(_) | I32(_) | U64(_) | I64(_) | U128(_) | I128(_)
            | U256(_) | I256(_) | String(_) | AV(_) => false,
            UniqueBool(_) | UniqueU8(_) | UniqueI8(_) | UniqueU16(_) | UniqueI16(_) | UniqueU32(_) | UniqueI32(_)
            | UniqueU64(_) | UniqueI64(_) | UniqueU128(_) | UniqueI128(_) | UniqueU256(_) | UniqueI256(_)
            | UniqueString(_) | UniqueAV(_) | UniqueDirectU8(_) | UniqueDirectU16(_) | UniqueDirectU32(_)
            | UniqueDirectU64(_) => true,
        }
    }

    /// Add the row referred to by `row_ref` to the index `self`,
    /// which must be keyed at `cols`.
    ///
    /// The returned `usize` is the number of bytes used by the key.
    /// [`BTreeIndex::check_and_insert`] will use this
    /// to update the counter for [`BTreeIndex::num_key_bytes`].
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
        match self {
            Self::Bool(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::U8(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::I8(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::U16(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::I16(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::U32(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::I32(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::U64(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::I64(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::U128(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::I128(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::U256(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::I256(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::String(idx) => mm_insert_at_type(idx, cols, row_ref),
            Self::AV(this) => {
                // SAFETY: Caller promised that any `col` in `cols` is in-bounds of `row_ref`'s layout.
                let key = unsafe { row_ref.project_unchecked(cols) };
                let key_size = key.key_size_in_bytes();
                this.insert(key, row_ref.pointer());
                Ok(key_size)
            }
            Self::UniqueBool(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueU8(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueI8(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueU16(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueI16(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueU32(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueI32(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueU64(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueI64(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueU128(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueI128(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueU256(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueI256(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueString(idx) => um_insert_at_type(idx, cols, row_ref),
            Self::UniqueAV(this) => {
                // SAFETY: Caller promised that any `col` in `cols` is in-bounds of `row_ref`'s layout.
                let key = unsafe { row_ref.project_unchecked(cols) };
                let key_size = key.key_size_in_bytes();
                this.insert(key, row_ref.pointer())
                    .map_err(|ptr| *ptr)
                    .map(|_| key_size)
            }
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
    /// [`BTreeIndex::delete`] will use this
    /// to update the counter for [`BTreeIndex::num_key_bytes`].
    /// We want to store said counter outside of the [`TypedIndex`] enum,
    /// but we can only compute the size using type info within the [`TypedIndex`],
    /// so we have to return the size across this boundary.
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

        match self {
            Self::Bool(this) => mm_delete_at_type(this, cols, row_ref),
            Self::U8(this) => mm_delete_at_type(this, cols, row_ref),
            Self::I8(this) => mm_delete_at_type(this, cols, row_ref),
            Self::U16(this) => mm_delete_at_type(this, cols, row_ref),
            Self::I16(this) => mm_delete_at_type(this, cols, row_ref),
            Self::U32(this) => mm_delete_at_type(this, cols, row_ref),
            Self::I32(this) => mm_delete_at_type(this, cols, row_ref),
            Self::U64(this) => mm_delete_at_type(this, cols, row_ref),
            Self::I64(this) => mm_delete_at_type(this, cols, row_ref),
            Self::U128(this) => mm_delete_at_type(this, cols, row_ref),
            Self::I128(this) => mm_delete_at_type(this, cols, row_ref),
            Self::U256(this) => mm_delete_at_type(this, cols, row_ref),
            Self::I256(this) => mm_delete_at_type(this, cols, row_ref),
            Self::String(this) => mm_delete_at_type(this, cols, row_ref),
            Self::AV(this) => {
                let key = row_ref.project(cols)?;
                let key_size = key.key_size_in_bytes();
                Ok(this.delete(&key, &row_ref.pointer()).then_some(key_size))
            }
            Self::UniqueBool(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueU8(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueI8(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueU16(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueI16(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueU32(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueI32(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueU64(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueI64(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueU128(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueI128(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueU256(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueI256(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueString(this) => um_delete_at_type(this, cols, row_ref),
            Self::UniqueAV(this) => {
                let key = row_ref.project(cols)?;
                let key_size = key.key_size_in_bytes();
                Ok(this.delete(&key).then_some(key_size))
            }
            Self::UniqueDirectU8(this) => direct_delete_at_type(this, cols, row_ref, |k: u8| k as usize),
            Self::UniqueDirectU16(this) => direct_delete_at_type(this, cols, row_ref, |k: u16| k as usize),
            Self::UniqueDirectU32(this) => direct_delete_at_type(this, cols, row_ref, |k: u32| k as usize),
            Self::UniqueDirectU64(this) => direct_delete_at_type(this, cols, row_ref, |k: u64| k as usize),
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
            Self::Bool(this) => Bool(mm_iter_at_type(this, range, AlgebraicValue::as_bool)),
            Self::U8(this) => U8(mm_iter_at_type(this, range, AlgebraicValue::as_u8)),
            Self::I8(this) => I8(mm_iter_at_type(this, range, AlgebraicValue::as_i8)),
            Self::U16(this) => U16(mm_iter_at_type(this, range, AlgebraicValue::as_u16)),
            Self::I16(this) => I16(mm_iter_at_type(this, range, AlgebraicValue::as_i16)),
            Self::U32(this) => U32(mm_iter_at_type(this, range, AlgebraicValue::as_u32)),
            Self::I32(this) => I32(mm_iter_at_type(this, range, AlgebraicValue::as_i32)),
            Self::U64(this) => U64(mm_iter_at_type(this, range, AlgebraicValue::as_u64)),
            Self::I64(this) => I64(mm_iter_at_type(this, range, AlgebraicValue::as_i64)),
            Self::U128(this) => U128(mm_iter_at_type(this, range, AlgebraicValue::as_u128)),
            Self::I128(this) => I128(mm_iter_at_type(this, range, AlgebraicValue::as_i128)),
            Self::U256(this) => U256(mm_iter_at_type(this, range, |av| av.as_u256().map(|x| &**x))),
            Self::I256(this) => I256(mm_iter_at_type(this, range, |av| av.as_i256().map(|x| &**x))),
            Self::String(this) => String(mm_iter_at_type(this, range, AlgebraicValue::as_string)),
            Self::AV(this) => AV(this.values_in_range(range)),

            Self::UniqueBool(this) => UniqueBool(um_iter_at_type(this, range, AlgebraicValue::as_bool)),
            Self::UniqueU8(this) => UniqueU8(um_iter_at_type(this, range, AlgebraicValue::as_u8)),
            Self::UniqueI8(this) => UniqueI8(um_iter_at_type(this, range, AlgebraicValue::as_i8)),
            Self::UniqueU16(this) => UniqueU16(um_iter_at_type(this, range, AlgebraicValue::as_u16)),
            Self::UniqueI16(this) => UniqueI16(um_iter_at_type(this, range, AlgebraicValue::as_i16)),
            Self::UniqueU32(this) => UniqueU32(um_iter_at_type(this, range, AlgebraicValue::as_u32)),
            Self::UniqueI32(this) => UniqueI32(um_iter_at_type(this, range, AlgebraicValue::as_i32)),
            Self::UniqueU64(this) => UniqueU64(um_iter_at_type(this, range, AlgebraicValue::as_u64)),
            Self::UniqueI64(this) => UniqueI64(um_iter_at_type(this, range, AlgebraicValue::as_i64)),
            Self::UniqueU128(this) => UniqueU128(um_iter_at_type(this, range, AlgebraicValue::as_u128)),
            Self::UniqueI128(this) => UniqueI128(um_iter_at_type(this, range, AlgebraicValue::as_i128)),
            Self::UniqueU256(this) => UniqueU256(um_iter_at_type(this, range, |av| av.as_u256().map(|x| &**x))),
            Self::UniqueI256(this) => UniqueI256(um_iter_at_type(this, range, |av| av.as_i256().map(|x| &**x))),
            Self::UniqueString(this) => UniqueString(um_iter_at_type(this, range, AlgebraicValue::as_string)),
            Self::UniqueAV(this) => UniqueAV(this.values_in_range(range)),

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
            Self::Bool(this) => this.clear(),
            Self::U8(this) => this.clear(),
            Self::I8(this) => this.clear(),
            Self::U16(this) => this.clear(),
            Self::I16(this) => this.clear(),
            Self::U32(this) => this.clear(),
            Self::I32(this) => this.clear(),
            Self::U64(this) => this.clear(),
            Self::I64(this) => this.clear(),
            Self::U128(this) => this.clear(),
            Self::I128(this) => this.clear(),
            Self::U256(this) => this.clear(),
            Self::I256(this) => this.clear(),
            Self::String(this) => this.clear(),
            Self::AV(this) => this.clear(),

            Self::UniqueBool(this) => this.clear(),
            Self::UniqueU8(this) => this.clear(),
            Self::UniqueI8(this) => this.clear(),
            Self::UniqueU16(this) => this.clear(),
            Self::UniqueI16(this) => this.clear(),
            Self::UniqueU32(this) => this.clear(),
            Self::UniqueI32(this) => this.clear(),
            Self::UniqueU64(this) => this.clear(),
            Self::UniqueI64(this) => this.clear(),
            Self::UniqueU128(this) => this.clear(),
            Self::UniqueI128(this) => this.clear(),
            Self::UniqueU256(this) => this.clear(),
            Self::UniqueI256(this) => this.clear(),
            Self::UniqueString(this) => this.clear(),
            Self::UniqueAV(this) => this.clear(),

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
            Self::Bool(this) => this.len(),
            Self::U8(this) => this.len(),
            Self::I8(this) => this.len(),
            Self::U16(this) => this.len(),
            Self::I16(this) => this.len(),
            Self::U32(this) => this.len(),
            Self::I32(this) => this.len(),
            Self::U64(this) => this.len(),
            Self::I64(this) => this.len(),
            Self::U128(this) => this.len(),
            Self::I128(this) => this.len(),
            Self::U256(this) => this.len(),
            Self::I256(this) => this.len(),
            Self::String(this) => this.len(),
            Self::AV(this) => this.len(),

            Self::UniqueBool(this) => this.len(),
            Self::UniqueU8(this) => this.len(),
            Self::UniqueI8(this) => this.len(),
            Self::UniqueU16(this) => this.len(),
            Self::UniqueI16(this) => this.len(),
            Self::UniqueU32(this) => this.len(),
            Self::UniqueI32(this) => this.len(),
            Self::UniqueU64(this) => this.len(),
            Self::UniqueI64(this) => this.len(),
            Self::UniqueU128(this) => this.len(),
            Self::UniqueI128(this) => this.len(),
            Self::UniqueU256(this) => this.len(),
            Self::UniqueI256(this) => this.len(),
            Self::UniqueString(this) => this.len(),
            Self::UniqueAV(this) => this.len(),

            Self::UniqueDirectU8(this)
            | Self::UniqueDirectU16(this)
            | Self::UniqueDirectU32(this)
            | Self::UniqueDirectU64(this) => this.len(),
        }
    }

    fn num_keys(&self) -> usize {
        match self {
            Self::Bool(this) => this.num_keys(),
            Self::U8(this) => this.num_keys(),
            Self::I8(this) => this.num_keys(),
            Self::U16(this) => this.num_keys(),
            Self::I16(this) => this.num_keys(),
            Self::U32(this) => this.num_keys(),
            Self::I32(this) => this.num_keys(),
            Self::U64(this) => this.num_keys(),
            Self::I64(this) => this.num_keys(),
            Self::U128(this) => this.num_keys(),
            Self::I128(this) => this.num_keys(),
            Self::U256(this) => this.num_keys(),
            Self::I256(this) => this.num_keys(),
            Self::String(this) => this.num_keys(),
            Self::AV(this) => this.num_keys(),

            Self::UniqueBool(this) => this.num_keys(),
            Self::UniqueU8(this) => this.num_keys(),
            Self::UniqueI8(this) => this.num_keys(),
            Self::UniqueU16(this) => this.num_keys(),
            Self::UniqueI16(this) => this.num_keys(),
            Self::UniqueU32(this) => this.num_keys(),
            Self::UniqueI32(this) => this.num_keys(),
            Self::UniqueU64(this) => this.num_keys(),
            Self::UniqueI64(this) => this.num_keys(),
            Self::UniqueU128(this) => this.num_keys(),
            Self::UniqueI128(this) => this.num_keys(),
            Self::UniqueU256(this) => this.num_keys(),
            Self::UniqueI256(this) => this.num_keys(),
            Self::UniqueString(this) => this.num_keys(),
            Self::UniqueAV(this) => this.num_keys(),

            Self::UniqueDirectU8(this)
            | Self::UniqueDirectU16(this)
            | Self::UniqueDirectU32(this)
            | Self::UniqueDirectU64(this) => this.num_keys(),
        }
    }
}

/// A B-Tree based index on a set of [`ColId`]s of a table.
#[derive(Debug, PartialEq, Eq)]
pub struct BTreeIndex {
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

impl MemoryUsage for BTreeIndex {
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

static_assert_size!(BTreeIndex, 88);

impl BTreeIndex {
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
        self.seek_range(value).next().is_some()
    }

    /// Returns the number of rows associated with this `value`.
    /// Returns `None` if 0.
    /// Returns `Some(1)` if the index is unique.
    pub fn count(&self, value: &AlgebraicValue) -> Option<usize> {
        match self.seek_range(value).count() {
            0 => None,
            n => Some(n),
        }
    }

    /// Returns an iterator over the [BTreeIndex] that yields all the `RowPointer`s
    /// that fall within the specified `range`.
    pub fn seek_range(&self, range: &impl RangeBounds<AlgebraicValue>) -> BTreeIndexRangeIter<'_> {
        BTreeIndexRangeIter {
            iter: self.idx.seek_range(range),
        }
    }

    /// Extends [`BTreeIndex`] with `rows`.
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

    /// Deletes all entries from the index, leaving it empty.
    ///
    /// When inserting a newly-created index into the committed state,
    /// we clear the tx state's index and insert it,
    /// rather than constructing a new `BTreeIndex`.
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

    fn new_index(row_type: &ProductType, cols: &ColList, is_unique: bool) -> BTreeIndex {
        let algo = BTreeAlgorithm { columns: cols.clone() }.into();
        BTreeIndex::new(row_type, &algo, is_unique).unwrap()
    }

    /// Extracts from `row` the relevant column values according to what columns are indexed.
    fn get_fields(cols: &ColList, row: &ProductValue) -> AlgebraicValue {
        row.project(cols).unwrap()
    }

    /// Returns whether indexing `row` again would violate a unique constraint, if any.
    fn violates_unique_constraint(index: &BTreeIndex, cols: &ColList, row: &ProductValue) -> bool {
        !index.is_unique() || index.contains_any(&get_fields(cols, row))
    }

    /// Returns an iterator over the rows that would violate the unique constraint of this index,
    /// if `row` were inserted,
    /// or `None`, if this index doesn't have a unique constraint.
    fn get_rows_that_violate_unique_constraint<'a>(
        index: &'a BTreeIndex,
        row: &'a AlgebraicValue,
    ) -> Option<BTreeIndexRangeIter<'a>> {
        index.is_unique().then(|| index.seek_range(row))
    }

    proptest! {
        #![proptest_config(ProptestConfig { max_shrink_iters: 0x10000000, ..Default::default() })]
        #[test]
        fn remove_nonexistent_noop(((ty, cols, pv), is_unique) in (gen_row_and_cols(), any::<bool>())) {
            let mut index = new_index(&ty, &cols, is_unique);
            let mut table = table(ty);
            let mut blob_store = HashMapBlobStore::default();
            let row_ref = table.insert(&mut blob_store, &pv).unwrap().1;
            prop_assert_eq!(index.delete(row_ref).unwrap(), false);
            prop_assert!(index.idx.is_empty());
        }

        #[test]
        fn insert_delete_noop(((ty, cols, pv), is_unique) in (gen_row_and_cols(), any::<bool>())) {
            let mut index = new_index(&ty, &cols, is_unique);
            let mut table = table(ty);
            let mut blob_store = HashMapBlobStore::default();
            let row_ref = table.insert(&mut blob_store, &pv).unwrap().1;
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
            let mut blob_store = HashMapBlobStore::default();
            let row_ref = table.insert(&mut blob_store, &pv).unwrap().1;
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
            let mut blob_store = HashMapBlobStore::default();

            let prev = needle - 1;
            let next = needle + 1;
            let range = prev..=next;

            let mut val_to_ptr = HashMap::default();

            // Insert `prev`, `needle`, and `next`.
            for x in range.clone() {
                let row = product![x];
                let row_ref = table.insert(&mut blob_store, &row).unwrap().1;
                val_to_ptr.insert(x, row_ref.pointer());
                // SAFETY: `row_ref` has the same type as was passed in when constructing `index`.
                prop_assert_eq!(unsafe { index.check_and_insert(row_ref) }, Ok(()));
            }

            fn test_seek(index: &BTreeIndex, val_to_ptr: &HashMap<u64, RowPointer>, range: impl RangeBounds<AlgebraicValue>, expect: impl IntoIterator<Item = u64>) -> TestCaseResult {
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
