//! BTree indexes with specialized key types.
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

use super::indexes::RowPointer;
use super::table::RowRef;
use crate::{
    layout::{AlgebraicTypeLayout, RowTypeLayout},
    read_column::ReadColumn,
    static_assert_size,
};
use core::ops::RangeBounds;
use multimap::{MultiMap, MultiMapRangeIter};
use spacetimedb_primitives::{ColList, IndexId};
use spacetimedb_sats::{algebraic_value::Packed, product_value::InvalidFieldError, AlgebraicValue};

mod multimap;

/// An iterator over a [`TypedMultiMap`], with a specialized key type.
///
/// See module docs for info about specialization.
enum TypedMultiMapRangeIter<'a> {
    Bool(MultiMapRangeIter<'a, bool, RowPointer>),
    U8(MultiMapRangeIter<'a, u8, RowPointer>),
    I8(MultiMapRangeIter<'a, i8, RowPointer>),
    U16(MultiMapRangeIter<'a, u16, RowPointer>),
    I16(MultiMapRangeIter<'a, i16, RowPointer>),
    U32(MultiMapRangeIter<'a, u32, RowPointer>),
    I32(MultiMapRangeIter<'a, i32, RowPointer>),
    U64(MultiMapRangeIter<'a, u64, RowPointer>),
    I64(MultiMapRangeIter<'a, i64, RowPointer>),
    U128(MultiMapRangeIter<'a, Packed<u128>, RowPointer>),
    I128(MultiMapRangeIter<'a, Packed<i128>, RowPointer>),
    String(MultiMapRangeIter<'a, Box<str>, RowPointer>),
    AlgebraicValue(MultiMapRangeIter<'a, AlgebraicValue, RowPointer>),
}

impl<'a> Iterator for TypedMultiMapRangeIter<'a> {
    type Item = RowPointer;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            TypedMultiMapRangeIter::Bool(ref mut this) => this.next(),
            TypedMultiMapRangeIter::U8(ref mut this) => this.next(),
            TypedMultiMapRangeIter::I8(ref mut this) => this.next(),
            TypedMultiMapRangeIter::U16(ref mut this) => this.next(),
            TypedMultiMapRangeIter::I16(ref mut this) => this.next(),
            TypedMultiMapRangeIter::U32(ref mut this) => this.next(),
            TypedMultiMapRangeIter::I32(ref mut this) => this.next(),
            TypedMultiMapRangeIter::U64(ref mut this) => this.next(),
            TypedMultiMapRangeIter::I64(ref mut this) => this.next(),
            TypedMultiMapRangeIter::U128(ref mut this) => this.next(),
            TypedMultiMapRangeIter::I128(ref mut this) => this.next(),
            TypedMultiMapRangeIter::String(ref mut this) => this.next(),
            TypedMultiMapRangeIter::AlgebraicValue(ref mut this) => this.next(),
        }
        .copied()
    }
}

/// An iterator over rows matching a certain [`AlgebraicValue`] on the [`BTreeIndex`].
pub struct BTreeIndexRangeIter<'a> {
    /// The iterator seeking for matching values.
    iter: TypedMultiMapRangeIter<'a>,
    /// The number of pointers yielded thus far.
    num_pointers_yielded: u64,
}

impl Iterator for BTreeIndexRangeIter<'_> {
    type Item = RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|ptr| {
            self.num_pointers_yielded += 1;
            ptr
        })
    }
}

impl BTreeIndexRangeIter<'_> {
    /// Returns the current number of pointers the iterator has returned thus far.
    pub fn num_pointers_yielded(&self) -> u64 {
        self.num_pointers_yielded
    }
}

/// A `MultiMap` from a key type determined at runtime to `RowPointer`.
///
/// See module docs for info about specialization.
enum TypedIndex {
    Bool(MultiMap<bool, RowPointer>),
    U8(MultiMap<u8, RowPointer>),
    I8(MultiMap<i8, RowPointer>),
    U16(MultiMap<u16, RowPointer>),
    I16(MultiMap<i16, RowPointer>),
    U32(MultiMap<u32, RowPointer>),
    I32(MultiMap<i32, RowPointer>),
    U64(MultiMap<u64, RowPointer>),
    I64(MultiMap<i64, RowPointer>),
    U128(MultiMap<Packed<u128>, RowPointer>),
    I128(MultiMap<Packed<i128>, RowPointer>),
    String(MultiMap<Box<str>, RowPointer>),
    AlgebraicValue(MultiMap<AlgebraicValue, RowPointer>),
}

impl TypedIndex {
    /// Add the row referred to by `row_ref` to the index `self`,
    /// which must be keyed at `cols`.
    ///
    /// If `cols` is inconsistent with `self`,
    /// or the `row_ref` has a row type other than that used for `self`,
    /// this will behave oddly; it may return an error,
    /// or may insert a nonsense value into the index.
    /// Note, however, that it will not invoke undefined behavior.
    fn insert(&mut self, cols: &ColList, row_ref: RowRef<'_>) -> Result<(), InvalidFieldError> {
        fn insert_at_type<T: Ord + ReadColumn>(
            this: &mut MultiMap<T, RowPointer>,
            cols: &ColList,
            row_ref: RowRef<'_>,
        ) -> Result<(), InvalidFieldError> {
            debug_assert!(cols.is_singleton());
            let col_pos = cols.head();
            let key = row_ref.read_col(col_pos).map_err(|_| col_pos)?;
            this.insert(key, row_ref.pointer());
            Ok(())
        }
        match self {
            TypedIndex::Bool(this) => insert_at_type(this, cols, row_ref),
            TypedIndex::U8(this) => insert_at_type(this, cols, row_ref),
            TypedIndex::I8(this) => insert_at_type(this, cols, row_ref),
            TypedIndex::U16(this) => insert_at_type(this, cols, row_ref),
            TypedIndex::I16(this) => insert_at_type(this, cols, row_ref),
            TypedIndex::U32(this) => insert_at_type(this, cols, row_ref),
            TypedIndex::I32(this) => insert_at_type(this, cols, row_ref),
            TypedIndex::U64(this) => insert_at_type(this, cols, row_ref),
            TypedIndex::I64(this) => insert_at_type(this, cols, row_ref),
            TypedIndex::U128(this) => insert_at_type(this, cols, row_ref),
            TypedIndex::I128(this) => insert_at_type(this, cols, row_ref),
            TypedIndex::String(this) => insert_at_type(this, cols, row_ref),

            TypedIndex::AlgebraicValue(this) => {
                let key = row_ref.project_not_empty(cols)?;
                this.insert(key, row_ref.pointer());
                Ok(())
            }
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
    fn delete(&mut self, cols: &ColList, row_ref: RowRef<'_>) -> Result<bool, InvalidFieldError> {
        fn delete_at_type<T: Ord + ReadColumn>(
            this: &mut MultiMap<T, RowPointer>,
            cols: &ColList,
            row_ref: RowRef<'_>,
        ) -> Result<bool, InvalidFieldError> {
            debug_assert!(cols.is_singleton());
            let col_pos = cols.head();
            let key = row_ref.read_col(col_pos).map_err(|_| col_pos)?;
            Ok(this.delete(&key, &row_ref.pointer()))
        }

        match self {
            TypedIndex::Bool(ref mut this) => delete_at_type(this, cols, row_ref),

            TypedIndex::U8(ref mut this) => delete_at_type(this, cols, row_ref),
            TypedIndex::I8(ref mut this) => delete_at_type(this, cols, row_ref),
            TypedIndex::U16(ref mut this) => delete_at_type(this, cols, row_ref),
            TypedIndex::I16(ref mut this) => delete_at_type(this, cols, row_ref),
            TypedIndex::U32(ref mut this) => delete_at_type(this, cols, row_ref),
            TypedIndex::I32(ref mut this) => delete_at_type(this, cols, row_ref),
            TypedIndex::U64(ref mut this) => delete_at_type(this, cols, row_ref),
            TypedIndex::I64(ref mut this) => delete_at_type(this, cols, row_ref),
            TypedIndex::U128(ref mut this) => delete_at_type(this, cols, row_ref),
            TypedIndex::I128(ref mut this) => delete_at_type(this, cols, row_ref),
            TypedIndex::String(ref mut this) => delete_at_type(this, cols, row_ref),

            TypedIndex::AlgebraicValue(ref mut this) => {
                let key = row_ref.project_not_empty(cols)?;
                Ok(this.delete(&key, &row_ref.pointer()))
            }
        }
    }

    fn values_in_range(&self, range: &impl RangeBounds<AlgebraicValue>) -> TypedMultiMapRangeIter<'_> {
        fn iter_at_type<'a, T: Ord>(
            this: &'a MultiMap<T, RowPointer>,
            range: &impl RangeBounds<AlgebraicValue>,
            av_as_t: impl Fn(&AlgebraicValue) -> Option<&T>,
        ) -> MultiMapRangeIter<'a, T, RowPointer> {
            use std::ops::Bound;
            let start = match range.start_bound() {
                Bound::Included(v) => {
                    Bound::Included(av_as_t(v).expect("Start bound of range does not conform to key type of index"))
                }
                Bound::Excluded(v) => {
                    Bound::Excluded(av_as_t(v).expect("Start bound of range does not conform to key type of index"))
                }
                Bound::Unbounded => Bound::Unbounded,
            };
            let end = match range.end_bound() {
                Bound::Included(v) => {
                    Bound::Included(av_as_t(v).expect("End bound of range does not conform to key type of index"))
                }
                Bound::Excluded(v) => {
                    Bound::Excluded(av_as_t(v).expect("End bound of range does not conform to key type of index"))
                }
                Bound::Unbounded => Bound::Unbounded,
            };
            this.values_in_range(&(start, end))
        }
        match self {
            TypedIndex::Bool(ref this) => {
                TypedMultiMapRangeIter::Bool(iter_at_type(this, range, AlgebraicValue::as_bool))
            }

            TypedIndex::U8(ref this) => TypedMultiMapRangeIter::U8(iter_at_type(this, range, AlgebraicValue::as_u8)),
            TypedIndex::I8(ref this) => TypedMultiMapRangeIter::I8(iter_at_type(this, range, AlgebraicValue::as_i8)),
            TypedIndex::U16(ref this) => TypedMultiMapRangeIter::U16(iter_at_type(this, range, AlgebraicValue::as_u16)),
            TypedIndex::I16(ref this) => TypedMultiMapRangeIter::I16(iter_at_type(this, range, AlgebraicValue::as_i16)),
            TypedIndex::U32(ref this) => TypedMultiMapRangeIter::U32(iter_at_type(this, range, AlgebraicValue::as_u32)),
            TypedIndex::I32(ref this) => TypedMultiMapRangeIter::I32(iter_at_type(this, range, AlgebraicValue::as_i32)),
            TypedIndex::U64(ref this) => TypedMultiMapRangeIter::U64(iter_at_type(this, range, AlgebraicValue::as_u64)),
            TypedIndex::I64(ref this) => TypedMultiMapRangeIter::I64(iter_at_type(this, range, AlgebraicValue::as_i64)),
            TypedIndex::U128(ref this) => {
                TypedMultiMapRangeIter::U128(iter_at_type(this, range, AlgebraicValue::as_u128))
            }
            TypedIndex::I128(ref this) => {
                TypedMultiMapRangeIter::I128(iter_at_type(this, range, AlgebraicValue::as_i128))
            }
            TypedIndex::String(ref this) => {
                TypedMultiMapRangeIter::String(iter_at_type(this, range, AlgebraicValue::as_string))
            }

            TypedIndex::AlgebraicValue(ref this) => TypedMultiMapRangeIter::AlgebraicValue(this.values_in_range(range)),
        }
    }

    fn clear(&mut self) {
        match self {
            TypedIndex::Bool(ref mut this) => this.clear(),
            TypedIndex::U8(ref mut this) => this.clear(),
            TypedIndex::I8(ref mut this) => this.clear(),
            TypedIndex::U16(ref mut this) => this.clear(),
            TypedIndex::I16(ref mut this) => this.clear(),
            TypedIndex::U32(ref mut this) => this.clear(),
            TypedIndex::I32(ref mut this) => this.clear(),
            TypedIndex::U64(ref mut this) => this.clear(),
            TypedIndex::I64(ref mut this) => this.clear(),
            TypedIndex::U128(ref mut this) => this.clear(),
            TypedIndex::I128(ref mut this) => this.clear(),
            TypedIndex::String(ref mut this) => this.clear(),
            TypedIndex::AlgebraicValue(ref mut this) => this.clear(),
        }
    }

    #[allow(unused)] // used only by tests
    fn is_empty(&self) -> bool {
        match self {
            TypedIndex::Bool(ref this) => this.is_empty(),
            TypedIndex::U8(ref this) => this.is_empty(),
            TypedIndex::I8(ref this) => this.is_empty(),
            TypedIndex::U16(ref this) => this.is_empty(),
            TypedIndex::I16(ref this) => this.is_empty(),
            TypedIndex::U32(ref this) => this.is_empty(),
            TypedIndex::I32(ref this) => this.is_empty(),
            TypedIndex::U64(ref this) => this.is_empty(),
            TypedIndex::I64(ref this) => this.is_empty(),
            TypedIndex::U128(ref this) => this.is_empty(),
            TypedIndex::I128(ref this) => this.is_empty(),
            TypedIndex::String(ref this) => this.is_empty(),
            TypedIndex::AlgebraicValue(ref this) => this.is_empty(),
        }
    }

    #[allow(unused)] // used only by tests
    fn len(&self) -> usize {
        match self {
            TypedIndex::Bool(ref this) => this.len(),
            TypedIndex::U8(ref this) => this.len(),
            TypedIndex::I8(ref this) => this.len(),
            TypedIndex::U16(ref this) => this.len(),
            TypedIndex::I16(ref this) => this.len(),
            TypedIndex::U32(ref this) => this.len(),
            TypedIndex::I32(ref this) => this.len(),
            TypedIndex::U64(ref this) => this.len(),
            TypedIndex::I64(ref this) => this.len(),
            TypedIndex::U128(ref this) => this.len(),
            TypedIndex::I128(ref this) => this.len(),
            TypedIndex::String(ref this) => this.len(),
            TypedIndex::AlgebraicValue(ref this) => this.len(),
        }
    }

    fn num_keys(&self) -> usize {
        match self {
            TypedIndex::Bool(ref this) => this.num_keys(),
            TypedIndex::U8(ref this) => this.num_keys(),
            TypedIndex::I8(ref this) => this.num_keys(),
            TypedIndex::U16(ref this) => this.num_keys(),
            TypedIndex::I16(ref this) => this.num_keys(),
            TypedIndex::U32(ref this) => this.num_keys(),
            TypedIndex::I32(ref this) => this.num_keys(),
            TypedIndex::U64(ref this) => this.num_keys(),
            TypedIndex::I64(ref this) => this.num_keys(),
            TypedIndex::U128(ref this) => this.num_keys(),
            TypedIndex::I128(ref this) => this.num_keys(),
            TypedIndex::String(ref this) => this.num_keys(),
            TypedIndex::AlgebraicValue(ref this) => this.num_keys(),
        }
    }
}

/// A B-Tree based index on a set of [`ColId`]s of a table.
pub struct BTreeIndex {
    /// The ID of this index.
    pub index_id: IndexId,
    /// Whether this index is also a unique constraint.
    pub(crate) is_unique: bool,
    /// The actual index, specialized for the appropriate key type.
    idx: TypedIndex,
}

static_assert_size!(BTreeIndex, 40);

impl BTreeIndex {
    /// Returns a new possibly unique index, with `index_id` for a set of columns.
    pub fn new(
        index_id: IndexId,
        row_type: &RowTypeLayout,
        indexed_columns: &ColList,
        is_unique: bool,
    ) -> Result<Self, InvalidFieldError> {
        // If the index is on a single column of a primitive type,
        // use a homogeneous map with a native key type.
        let typed_index = if indexed_columns.is_singleton() {
            let col_pos = indexed_columns.head();
            let col = row_type.product().elements.get(col_pos.idx()).ok_or(col_pos)?;

            match col.ty {
                AlgebraicTypeLayout::Bool => TypedIndex::Bool(MultiMap::new()),
                AlgebraicTypeLayout::I8 => TypedIndex::I8(MultiMap::new()),
                AlgebraicTypeLayout::U8 => TypedIndex::U8(MultiMap::new()),
                AlgebraicTypeLayout::I16 => TypedIndex::I16(MultiMap::new()),
                AlgebraicTypeLayout::U16 => TypedIndex::U16(MultiMap::new()),
                AlgebraicTypeLayout::I32 => TypedIndex::I32(MultiMap::new()),
                AlgebraicTypeLayout::U32 => TypedIndex::U32(MultiMap::new()),
                AlgebraicTypeLayout::I64 => TypedIndex::I64(MultiMap::new()),
                AlgebraicTypeLayout::U64 => TypedIndex::U64(MultiMap::new()),
                AlgebraicTypeLayout::I128 => TypedIndex::I128(MultiMap::new()),
                AlgebraicTypeLayout::U128 => TypedIndex::U128(MultiMap::new()),
                AlgebraicTypeLayout::String => TypedIndex::String(MultiMap::new()),

                // If we don't specialize on the key type, use a map keyed on `AlgebraicValue`.
                _ => TypedIndex::AlgebraicValue(MultiMap::new()),
            }
        } else {
            // If the index is on multiple columns, use a map keyed on `AlgebraicValue`,
            // as the keys will be `ProductValue`s.
            TypedIndex::AlgebraicValue(MultiMap::new())
        };
        Ok(Self {
            index_id,
            is_unique,
            idx: typed_index,
        })
    }

    /// Inserts `ptr` with the value `row` to this index.
    /// This index will extract the necessary values from `row` based on `self.cols`.
    ///
    /// Return false if `ptr` was already indexed prior to this call.
    pub fn insert(&mut self, cols: &ColList, row_ref: RowRef<'_>) -> Result<(), InvalidFieldError> {
        self.idx.insert(cols, row_ref)
    }

    /// Deletes `ptr` with its indexed value `col_value` from this index.
    ///
    /// Returns whether `ptr` was present.
    pub fn delete(&mut self, cols: &ColList, row_ref: RowRef<'_>) -> Result<bool, InvalidFieldError> {
        self.idx.delete(cols, row_ref)
    }

    /// Returns an iterator over the rows that would violate the unique constraint of this index,
    /// if `row` were inserted,
    /// or `None`, if this index doesn't have a unique constraint.
    pub fn get_rows_that_violate_unique_constraint<'a>(
        &'a self,
        row: &'a AlgebraicValue,
    ) -> Option<BTreeIndexRangeIter<'a>> {
        self.is_unique.then(|| self.seek(row))
    }

    /// Returns whether `value` is in this index.
    pub fn contains_any(&self, value: &AlgebraicValue) -> bool {
        self.seek(value).next().is_some()
    }

    /// Returns an iterator over the [BTreeIndex] that yields all the `RowPointer`s
    /// that fall within the specified `range`.
    pub fn seek(&self, range: &impl RangeBounds<AlgebraicValue>) -> BTreeIndexRangeIter<'_> {
        BTreeIndexRangeIter {
            iter: self.idx.values_in_range(range),
            num_pointers_yielded: 0,
        }
    }

    /// Extends [`BTreeIndex`] with `rows`.
    /// Returns whether every element in `rows` was inserted.
    pub fn build_from_rows<'table>(
        &mut self,
        cols: &ColList,
        rows: impl IntoIterator<Item = RowRef<'table>>,
    ) -> Result<(), InvalidFieldError> {
        for row_ref in rows {
            self.insert(cols, row_ref)?;
        }
        Ok(())
    }

    /// Deletes all entries from the index, leaving it empty.
    ///
    /// When inserting a newly-created index into the committed state,
    /// we clear the tx state's index and insert it,
    /// rather than constructing a new `BTreeIndex`.
    pub fn clear(&mut self) {
        self.idx.clear();
    }

    /// The number of unique keys in this index.
    pub fn num_keys(&self) -> usize {
        self.idx.num_keys()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{blob_store::HashMapBlobStore, indexes::SquashedOffset, table::Table};
    use core::ops::Bound::*;
    use proptest::prelude::*;
    use proptest::{collection::vec, test_runner::TestCaseResult};
    use spacetimedb_data_structures::map::HashMap;
    use spacetimedb_primitives::ColListBuilder;
    use spacetimedb_sats::{
        db::def::{TableDef, TableSchema},
        product,
        proptest::{generate_product_value, generate_row_type},
        AlgebraicType, ProductType, ProductValue,
    };

    fn gen_cols(ty_len: usize) -> impl Strategy<Value = ColList> {
        vec((0..ty_len as u32).prop_map_into(), 1..=ty_len)
            .prop_map(|cols| cols.into_iter().collect::<ColListBuilder>().build().unwrap())
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
        let row_layout: RowTypeLayout = row_type.clone().into();
        BTreeIndex::new(0.into(), &row_layout, cols, is_unique).unwrap()
    }

    fn table(ty: ProductType) -> Table {
        let def = TableDef::from_product("", ty);
        let schema = TableSchema::from_def(0.into(), def);
        Table::new(schema.into(), SquashedOffset::COMMITTED_STATE)
    }

    /// Extracts from `row` the relevant column values according to what columns are indexed.
    fn get_fields(cols: &ColList, row: &ProductValue) -> AlgebraicValue {
        row.project_not_empty(cols).unwrap()
    }

    /// Returns whether indexing `row` again would violate a unique constraint, if any.
    fn violates_unique_constraint(index: &BTreeIndex, cols: &ColList, row: &ProductValue) -> bool {
        !index.is_unique || index.contains_any(&get_fields(cols, row))
    }

    proptest! {
        #[test]
        fn remove_nonexistent_noop(((ty, cols, pv), is_unique) in (gen_row_and_cols(), any::<bool>())) {
            let mut index = new_index(&ty, &cols, is_unique);
            let mut table = table(ty);
            let mut blob_store = HashMapBlobStore::default();
            let row_ref = table.insert(&mut blob_store, &pv).unwrap().1;
            prop_assert_eq!(index.delete(&cols, row_ref).unwrap(), false);
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

            index.insert(&cols, row_ref).unwrap();
            prop_assert_eq!(index.idx.len(), 1);
            prop_assert_eq!(index.contains_any(&value), true);

            prop_assert_eq!(index.delete(&cols, row_ref).unwrap(), true);
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
                index.get_rows_that_violate_unique_constraint(&value).unwrap().collect::<Vec<_>>(),
                []
            );

            // Insert.
            index.insert(&cols, row_ref).unwrap();

            // Inserting again would be a problem.
            prop_assert_eq!(index.idx.len(), 1);
            prop_assert_eq!(violates_unique_constraint(&index, &cols, &pv), true);
            prop_assert_eq!(
                index.get_rows_that_violate_unique_constraint(&value).unwrap().collect::<Vec<_>>(),
                [row_ref.pointer()]
            );
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

            let mut val_to_ptr = HashMap::new();

            // Insert `prev`, `needle`, and `next`.
            for x in range.clone() {
                let row = product![x];
                let row_ref = table.insert(&mut blob_store, &row).unwrap().1;
                val_to_ptr.insert(x, row_ref.pointer());
                index.insert(&cols, row_ref).unwrap();
            }

            fn test_seek(index: &BTreeIndex, val_to_ptr: &HashMap<u64, RowPointer>, range: impl RangeBounds<AlgebraicValue>, expect: impl IntoIterator<Item = u64>) -> TestCaseResult {
                let mut ptrs_in_index = index.seek(&range).collect::<Vec<_>>();
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
