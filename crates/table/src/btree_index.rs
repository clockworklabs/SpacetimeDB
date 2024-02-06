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
    static_assert_size,
};
use core::ops::RangeBounds;
use multimap::{MultiMap, MultiMapRangeIter};
use spacetimedb_primitives::{ColList, IndexId};
use spacetimedb_sats::{product_value::InvalidFieldError, AlgebraicValue, ProductValue};

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
    U128(MultiMapRangeIter<'a, u128, RowPointer>),
    I128(MultiMapRangeIter<'a, i128, RowPointer>),
    String(MultiMapRangeIter<'a, String, RowPointer>),
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
    U128(MultiMap<u128, RowPointer>),
    I128(MultiMap<i128, RowPointer>),
    String(MultiMap<String, RowPointer>),
    AlgebraicValue(MultiMap<AlgebraicValue, RowPointer>),
}

impl TypedIndex {
    // NOTE(pgoldman 2024-02-05): this method is structured the way it is,
    // taking the `cols` and `row` rather than the new key,
    // so that it will be amenable to rewriting in terms of `ReadColumn::read_column`,
    // once that PR lands.
    fn insert(&mut self, cols: &ColList, row: &ProductValue, ptr: RowPointer) -> Result<bool, InvalidFieldError> {
        fn insert_at_type<T: Clone + Ord>(
            this: &mut MultiMap<T, RowPointer>,
            cols: &ColList,
            row: &ProductValue,
            ptr: RowPointer,
            av_as_t: impl FnOnce(&AlgebraicValue) -> Option<&T>,
        ) -> Result<bool, InvalidFieldError> {
            debug_assert!(cols.is_singleton());
            let col_pos = cols.head();
            let key = row
                .elements
                .get(col_pos.idx())
                .and_then(av_as_t)
                .ok_or(InvalidFieldError { col_pos, name: None })?;
            Ok(this.insert(key.clone(), ptr))
        }
        match self {
            TypedIndex::Bool(ref mut this) => insert_at_type(this, cols, row, ptr, AlgebraicValue::as_bool),

            TypedIndex::U8(ref mut this) => insert_at_type(this, cols, row, ptr, AlgebraicValue::as_u8),
            TypedIndex::I8(ref mut this) => insert_at_type(this, cols, row, ptr, AlgebraicValue::as_i8),
            TypedIndex::U16(ref mut this) => insert_at_type(this, cols, row, ptr, AlgebraicValue::as_u16),
            TypedIndex::I16(ref mut this) => insert_at_type(this, cols, row, ptr, AlgebraicValue::as_i16),
            TypedIndex::U32(ref mut this) => insert_at_type(this, cols, row, ptr, AlgebraicValue::as_u32),
            TypedIndex::I32(ref mut this) => insert_at_type(this, cols, row, ptr, AlgebraicValue::as_i32),
            TypedIndex::U64(ref mut this) => insert_at_type(this, cols, row, ptr, AlgebraicValue::as_u64),
            TypedIndex::I64(ref mut this) => insert_at_type(this, cols, row, ptr, AlgebraicValue::as_i64),
            TypedIndex::U128(ref mut this) => insert_at_type(this, cols, row, ptr, AlgebraicValue::as_u128),
            TypedIndex::I128(ref mut this) => insert_at_type(this, cols, row, ptr, AlgebraicValue::as_i128),
            TypedIndex::String(ref mut this) => insert_at_type(this, cols, row, ptr, AlgebraicValue::as_string),

            TypedIndex::AlgebraicValue(ref mut this) => {
                let key = row.project_not_empty(cols)?;
                Ok(this.insert(key, ptr))
            }
        }
    }

    // NOTE(pgoldman 2024-02-05): this method is structured the way it is,
    // taking the `cols` and `row` rather than the sought key,
    // so that it will be amenable to rewriting in terms of `ReadColumn::read_column`,
    // once that PR lands.
    fn delete(&mut self, cols: &ColList, row: &ProductValue, ptr: RowPointer) -> Result<bool, InvalidFieldError> {
        fn delete_at_type<T: Ord>(
            this: &mut MultiMap<T, RowPointer>,

            cols: &ColList,
            row: &ProductValue,
            ptr: RowPointer,
            av_as_t: impl FnOnce(&AlgebraicValue) -> Option<&T>,
        ) -> Result<bool, InvalidFieldError> {
            debug_assert!(cols.is_singleton());
            let col_pos = cols.head();
            let key = row
                .elements
                .get(col_pos.idx())
                .and_then(av_as_t)
                .ok_or(InvalidFieldError { col_pos, name: None })?;
            Ok(this.delete(key, &ptr))
        }

        match self {
            TypedIndex::Bool(ref mut this) => delete_at_type(this, cols, row, ptr, AlgebraicValue::as_bool),

            TypedIndex::U8(ref mut this) => delete_at_type(this, cols, row, ptr, AlgebraicValue::as_u8),
            TypedIndex::I8(ref mut this) => delete_at_type(this, cols, row, ptr, AlgebraicValue::as_i8),
            TypedIndex::U16(ref mut this) => delete_at_type(this, cols, row, ptr, AlgebraicValue::as_u16),
            TypedIndex::I16(ref mut this) => delete_at_type(this, cols, row, ptr, AlgebraicValue::as_i16),
            TypedIndex::U32(ref mut this) => delete_at_type(this, cols, row, ptr, AlgebraicValue::as_u32),
            TypedIndex::I32(ref mut this) => delete_at_type(this, cols, row, ptr, AlgebraicValue::as_i32),
            TypedIndex::U64(ref mut this) => delete_at_type(this, cols, row, ptr, AlgebraicValue::as_u64),
            TypedIndex::I64(ref mut this) => delete_at_type(this, cols, row, ptr, AlgebraicValue::as_i64),
            TypedIndex::U128(ref mut this) => delete_at_type(this, cols, row, ptr, AlgebraicValue::as_u128),
            TypedIndex::I128(ref mut this) => delete_at_type(this, cols, row, ptr, AlgebraicValue::as_i128),
            TypedIndex::String(ref mut this) => delete_at_type(this, cols, row, ptr, AlgebraicValue::as_string),

            TypedIndex::AlgebraicValue(ref mut this) => {
                let key = row.project_not_empty(cols)?;
                Ok(this.delete(&key, &ptr))
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
}

/// A B-Tree based index on a set of [`ColId`]s of a table.
pub struct BTreeIndex {
    /// The ID of this index.
    pub index_id: IndexId,
    /// Whether this index is also a unique constraint.
    pub(crate) is_unique: bool,
    /// The actual index, specialized for the appropriate key type.
    idx: TypedIndex,
    /// The index name, used for reporting unique constraint violations.
    pub(crate) name: Box<str>,
}

static_assert_size!(BTreeIndex, 56);

impl BTreeIndex {
    /// Returns a new possibly unique index, with `index_id` for a set of columns.
    pub fn new(
        index_id: IndexId,
        row_type: &RowTypeLayout,
        indexed_columns: &ColList,
        is_unique: bool,
        name: impl Into<Box<str>>,
    ) -> Result<Self, InvalidFieldError> {
        // If the index is on a single column of a primitive type,
        // use a homogeneous map with a native key type.
        let typed_index = if indexed_columns.is_singleton() {
            let col_pos = indexed_columns.head().idx();
            let col = row_type.product().elements.get(col_pos).ok_or(InvalidFieldError {
                col_pos: col_pos.into(),
                name: None,
            })?;

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
            name: name.into(),
        })
    }

    /// Extracts from `row` the relevant column values according to what columns are indexed.
    pub fn get_fields(&self, cols: &ColList, row: &ProductValue) -> Result<AlgebraicValue, InvalidFieldError> {
        row.project_not_empty(cols)
    }

    /// Inserts `ptr` with the value `row` to this index.
    /// This index will extract the necessary values from `row` based on `self.cols`.
    ///
    /// Return false if `ptr` was already indexed prior to this call.
    pub fn insert(&mut self, cols: &ColList, row: &ProductValue, ptr: RowPointer) -> Result<bool, InvalidFieldError> {
        self.idx.insert(cols, row, ptr)
    }

    /// Deletes `ptr` with its indexed value `col_value` from this index.
    ///
    /// Returns whether `ptr` was present.
    pub fn delete(&mut self, cols: &ColList, row: &ProductValue, ptr: RowPointer) -> Result<bool, InvalidFieldError> {
        self.idx.delete(cols, row, ptr)
    }

    /// Returns whether indexing `row` again would violate a unique constraint, if any.
    pub fn violates_unique_constraint(&self, cols: &ColList, row: &ProductValue) -> bool {
        if self.is_unique {
            let col_value = self.get_fields(cols, row).unwrap();
            return self.contains_any(&col_value);
        }
        false
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
    ) -> Result<bool, InvalidFieldError> {
        let mut all_inserted = true;
        for row_ref in rows {
            let row = row_ref.to_product_value();
            all_inserted &= self.insert(cols, &row, row_ref.pointer())?;
        }
        Ok(all_inserted)
    }

    /// Deletes all entries from the index, leaving it empty.
    ///
    /// When inserting a newly-created index into the committed state,
    /// we clear the tx state's index and insert it,
    /// rather than constructing a new `BTreeIndex`.
    pub fn clear(&mut self) {
        self.idx.clear();
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        indexes::{PageIndex, PageOffset, SquashedOffset},
        proptest_sats::{generate_product_value, generate_row_type},
    };
    use core::ops::Bound::*;
    use proptest::prelude::*;
    use proptest::{collection::vec, test_runner::TestCaseResult};
    use spacetimedb_primitives::ColListBuilder;
    use spacetimedb_sats::{product, AlgebraicType, ProductType};

    fn gen_row_pointer() -> impl Strategy<Value = RowPointer> {
        (any::<PageOffset>(), any::<PageIndex>()).prop_map(|(po, pi)| RowPointer::new(false, pi, po, SquashedOffset(0)))
    }

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
        BTreeIndex::new(0.into(), &row_layout, cols, is_unique, "test_index").unwrap()
    }

    proptest! {
        #[test]
        fn remove_nonexistent_noop(((ty, cols, pv), ptr, is_unique) in (gen_row_and_cols(), gen_row_pointer(), any::<bool>())) {
            let mut index = new_index(&ty, &cols, is_unique);
            prop_assert_eq!(index.delete(&cols, &pv, ptr).unwrap(), false);
            prop_assert!(index.idx.is_empty());
        }

        #[test]
        fn insert_delete_noop(((ty, cols, pv), ptr, is_unique) in (gen_row_and_cols(), gen_row_pointer(), any::<bool>())) {
            let mut index = new_index(&ty, &cols, is_unique);
            let value = index.get_fields(&cols, &pv).unwrap();
            prop_assert_eq!(index.idx.len(), 0);
            prop_assert_eq!(index.contains_any(&value), false);

            prop_assert_eq!(index.insert(&cols, &pv, ptr).unwrap(), true);
            prop_assert_eq!(index.idx.len(), 1);
            prop_assert_eq!(index.contains_any(&value), true);

            // Try inserting again, it should fail.
            prop_assert_eq!(index.insert(&cols, &pv, ptr).unwrap(), false);
            prop_assert_eq!(index.idx.len(), 1);

            prop_assert_eq!(index.delete(&cols, &pv, ptr).unwrap(), true);
            prop_assert_eq!(index.idx.len(), 0);
            prop_assert_eq!(index.contains_any(&value), false);
        }

        #[test]
        fn insert_again_violates_unique_constraint(((ty, cols, pv), ptr) in (gen_row_and_cols(), gen_row_pointer())) {
            let mut index = new_index(&ty, &cols, true);
            let value = index.get_fields(&cols, &pv).unwrap();

            // Nothing in the index yet.
            prop_assert_eq!(index.idx.len(), 0);
            prop_assert_eq!(index.violates_unique_constraint(&cols, &pv), false);
            prop_assert_eq!(
                index.get_rows_that_violate_unique_constraint(&value).unwrap().collect::<Vec<_>>(),
                []
            );

            // Insert.
            prop_assert_eq!(index.insert(&cols, &pv, ptr).unwrap(), true);

            // Inserting again would be a problem.
            prop_assert_eq!(index.idx.len(), 1);
            prop_assert_eq!(index.violates_unique_constraint(&cols, &pv), true);
            prop_assert_eq!(
                index.get_rows_that_violate_unique_constraint(&value).unwrap().collect::<Vec<_>>(),
                [ptr]
            );
        }

        #[test]
        fn seek_various_ranges(needle in 1..u64::MAX) {
            use AlgebraicValue::U64 as V;

            let cols = 0.into();
            let mut index = new_index(&ProductType::from_iter([AlgebraicType::U64]), &cols, true);

            let prev = needle - 1;
            let next = needle + 1;
            let range = prev..=next;

            // Insert `prev`, `needle`, and `next`.
            for x in range.clone() {
                prop_assert_eq!(index.insert(&cols, &product![x], RowPointer(x)).unwrap(), true);
            }

            fn test_seek(index: &BTreeIndex, range: impl RangeBounds<AlgebraicValue>, expect: impl IntoIterator<Item = u64>) -> TestCaseResult {
                prop_assert_eq!(
                    index.seek(&range).collect::<Vec<_>>(),
                    expect.into_iter().map(RowPointer).collect::<Vec<_>>()
                );
                Ok(())
            }

            // Test point ranges.
            for x in range.clone() {
                test_seek(&index, V(x), [x])?;
            }

            // Test `..` (`RangeFull`).
            test_seek(&index, .., [prev, needle, next])?;

            // Test `x..` (`RangeFrom`).
            test_seek(&index, V(prev).., [prev, needle, next])?;
            test_seek(&index, V(needle).., [needle, next])?;
            test_seek(&index, V(next).., [next])?;

            // Test `..x` (`RangeTo`).
            test_seek(&index, ..V(prev), [])?;
            test_seek(&index, ..V(needle), [prev])?;
            test_seek(&index, ..V(next), [prev, needle])?;

            // Test `..=x` (`RangeToInclusive`).
            test_seek(&index, ..=V(prev), [prev])?;
            test_seek(&index, ..=V(needle), [prev, needle])?;
            test_seek(&index, ..=V(next), [prev, needle, next])?;

            // Test `x..y` (`Range`).
            test_seek(&index, V(prev)..V(prev), [])?;
            test_seek(&index, V(prev)..V(needle), [prev])?;
            test_seek(&index, V(prev)..V(next), [prev, needle])?;
            test_seek(&index, V(needle)..V(next), [needle])?;

            // Test `x..=y` (`RangeInclusive`).
            test_seek(&index, V(prev)..=V(prev), [prev])?;
            test_seek(&index, V(prev)..=V(needle), [prev, needle])?;
            test_seek(&index, V(prev)..=V(next), [prev, needle, next])?;
            test_seek(&index, V(needle)..=V(next), [needle, next])?;
            test_seek(&index, V(next)..=V(next), [next])?;

            // Test `(x, y]` (Exclusive start, inclusive end).
            test_seek(&index, (Excluded(V(prev)), Included(V(prev))), [])?;
            test_seek(&index, (Excluded(V(prev)), Included(V(needle))), [needle])?;
            test_seek(&index, (Excluded(V(prev)), Included(V(next))), [needle, next])?;

            // Test `(x, inf]` (Exclusive start, unbounded end).
            test_seek(&index, (Excluded(V(prev)), Unbounded), [needle, next])?;
            test_seek(&index, (Excluded(V(needle)), Unbounded), [next])?;
            test_seek(&index, (Excluded(V(next)), Unbounded), [])?;

            // Test `(x, y)` (Exclusive start, exclusive end).
            test_seek(&index, (Excluded(V(prev)), Excluded(V(needle))), [])?;
            test_seek(&index, (Excluded(V(prev)), Excluded(V(next))), [needle])?;
        }
    }
}
