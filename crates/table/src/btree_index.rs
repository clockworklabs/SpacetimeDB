use super::indexes::RowPointer;
use super::table::RowRef;
use crate::static_assert_size;
use core::ops::RangeBounds;
use multimap::{MultiMap, MultiMapRangeIter};
use spacetimedb_primitives::{ColList, IndexId};
use spacetimedb_sats::{product_value::InvalidFieldError, AlgebraicValue, ProductValue, ProductType, BuiltinType};

mod multimap;

/// An index key storing a mapping to rows via `RowPointer`s
/// as well as the value the rows have for the relevant [`ColId`]s.
///
/// ## Index Key Composition
///
/// `IndexKey` uses an [`AlgebraicValue`] to optimize for the common case of *single columns* as key.
///
/// See [`ProductValue::project`] for the logic.
///
/// ### SQL Examples
///
/// To illustrate the concept of single and multiple column indexes, consider the following SQL examples:
///
/// ```sql
/// CREATE INDEX a ON t1 (column_i32); -- Creating a single column index, a common case.
/// CREATE INDEX b ON t1 (column_i32, column_i32); -- Creating a multiple column index for more complex requirements.
/// ```
/// Will be on memory:
///
/// ```rust,ignore
/// [AlgebraicValue::I32(0)] = Row(ProductValue(...))
/// [AlgebraicValue::Product(AlgebraicValue::I32(0), AlgebraicValue::I32(1))] = Row(ProductValue(...))
/// ```
type IndexKey = AlgebraicValue;

/// An iterator over rows matching a certain [`AlgebraicValue`] on the [`BTreeIndex`].
pub struct BTreeIndexRangeIter<'a> {
    /// The iterator seeking for matching values.
    iter: RangeIterImpl<'a>,
    /// The number of pointers yielded thus far.
    num_pointers_yielded: u64,
}

impl Iterator for BTreeIndexRangeIter<'_> {
    type Item = RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|ptr| {
            self.num_pointers_yielded += 1;
            *ptr
        })
    }
}

impl BTreeIndexRangeIter<'_> {
    /// Returns the current number of pointers the iterator has returned thus far.
    ///
    // TODO: Consider renaming this to something less confusing.
    pub fn keys_scanned(&self) -> u64 {
        self.num_pointers_yielded
    }
}

enum RangeIterImpl<'a> {
    Bool(MultiMapRangeIter<'a, bool, RowPointer>),
    U8(MultiMapRangeIter<'a, u8, RowPointer>),
    U16(MultiMapRangeIter<'a, u16, RowPointer>),
    U32(MultiMapRangeIter<'a, u32, RowPointer>),
    U64(MultiMapRangeIter<'a, u64, RowPointer>),
    U128(MultiMapRangeIter<'a, u128, RowPointer>),
    I8(MultiMapRangeIter<'a, i8, RowPointer>),
    I16(MultiMapRangeIter<'a, i16, RowPointer>),
    I32(MultiMapRangeIter<'a, i32, RowPointer>),
    I64(MultiMapRangeIter<'a, i64, RowPointer>),
    I128(MultiMapRangeIter<'a, i128, RowPointer>),
    AlgebraicValue(MultiMapRangeIter<'a, AlgebraicValue, RowPointer>),
}

impl<'a> Iterator for RangeIterImpl<'a> {
    type Item = &'a RowPointer;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            RangeIterImpl::Bool(iter) => iter.next(),
            RangeIterImpl::U8(iter) => iter.next(),
            RangeIterImpl::U16(iter) => iter.next(),
            RangeIterImpl::U32(iter) => iter.next(),
            RangeIterImpl::U64(iter) => iter.next(),
            RangeIterImpl::U128(iter) => iter.next(),
            RangeIterImpl::I8(iter) => iter.next(),
            RangeIterImpl::I16(iter) => iter.next(),
            RangeIterImpl::I32(iter) => iter.next(),
            RangeIterImpl::I64(iter) => iter.next(),
            RangeIterImpl::I128(iter) => iter.next(),
            RangeIterImpl::AlgebraicValue(iter) => iter.next(),
        }
    }
}

/// A B-Tree based index on a set of [`ColId`]s of a table.
pub struct BTreeIndex {
    /// The ID of this index.
    pub(crate) index_id: IndexId,
    /// Whether this index is also a unique constraint.
    pub(crate) is_unique: bool,
    /// The actual index.
    idx: IdxImpl,
    /// The index name, used for reporting unique constraint violations.
    pub(crate) name: Box<str>,
}

enum IdxImpl {
    Bool(MultiMap<bool, RowPointer>),
    U8(MultiMap<u8, RowPointer>),
    U16(MultiMap<u16, RowPointer>),
    U32(MultiMap<u32, RowPointer>),
    U64(MultiMap<u64, RowPointer>),
    U128(MultiMap<u128, RowPointer>),
    I8(MultiMap<i8, RowPointer>),
    I16(MultiMap<i16, RowPointer>),
    I32(MultiMap<i32, RowPointer>),
    I64(MultiMap<i64, RowPointer>),
    I128(MultiMap<i128, RowPointer>),
    AlgebraicValue(MultiMap<IndexKey, RowPointer>),
}

impl IdxImpl {
    /// Inserts `ptr` with the value `row` to this index.
    /// This index will extract the necessary values from `row` based on `self.cols`.
    ///
    /// Return false if `ptr` was already indexed prior to this call.
    pub fn insert(&mut self, col_value: AlgebraicValue, ptr: RowPointer) -> Result<bool, InvalidFieldError> {
        Ok(match self {
            IdxImpl::Bool(idx) => idx.insert(*col_value.as_bool().unwrap(), ptr),
            IdxImpl::U8(idx) => idx.insert(*col_value.as_u8().unwrap(), ptr),
            IdxImpl::U16(idx) => idx.insert(*col_value.as_u16().unwrap(), ptr),
            IdxImpl::U32(idx) => idx.insert(*col_value.as_u32().unwrap(), ptr),
            IdxImpl::U64(idx) => idx.insert(*col_value.as_u64().unwrap(), ptr),
            IdxImpl::U128(idx) => idx.insert(*col_value.as_u128().unwrap(), ptr),
            IdxImpl::I8(idx) => idx.insert(*col_value.as_i8().unwrap(), ptr),
            IdxImpl::I16(idx) => idx.insert(*col_value.as_i16().unwrap(), ptr),
            IdxImpl::I32(idx) => idx.insert(*col_value.as_i32().unwrap(), ptr),
            IdxImpl::I64(idx) => idx.insert(*col_value.as_i64().unwrap(), ptr),
            IdxImpl::I128(idx) => idx.insert(*col_value.as_i128().unwrap(), ptr),
            IdxImpl::AlgebraicValue(idx) => idx.insert(col_value, ptr),
        })
    }

    /// Deletes `ptr` with its indexed value `col_value` from this index.
    ///
    /// Returns whether `ptr` was present.
    pub fn delete(&mut self, col_value: &AlgebraicValue, ptr: &RowPointer) -> bool {
        match self {
            IdxImpl::Bool(idx) => idx.delete(col_value.as_bool().unwrap(), ptr),
            IdxImpl::U8(idx) => idx.delete(col_value.as_u8().unwrap(), ptr),
            IdxImpl::U16(idx) => idx.delete(col_value.as_u16().unwrap(), ptr),
            IdxImpl::U32(idx) => idx.delete(col_value.as_u32().unwrap(), ptr),
            IdxImpl::U64(idx) => idx.delete(col_value.as_u64().unwrap(), ptr),
            IdxImpl::U128(idx) => idx.delete(col_value.as_u128().unwrap(), ptr),
            IdxImpl::I8(idx) => idx.delete(col_value.as_i8().unwrap(), ptr),
            IdxImpl::I16(idx) => idx.delete(col_value.as_i16().unwrap(), ptr),
            IdxImpl::I32(idx) => idx.delete(col_value.as_i32().unwrap(), ptr),
            IdxImpl::I64(idx) => idx.delete(col_value.as_i64().unwrap(), ptr),
            IdxImpl::I128(idx) => idx.delete(col_value.as_i128().unwrap(), ptr),
            IdxImpl::AlgebraicValue(idx) => idx.delete(col_value, ptr),
        }
    }

    /// Returns an iterator over the multimap that yields all the `V`s
    /// of the `K`s that fall within the specified `range`.
    pub fn values_in_range(&self, range: &impl RangeBounds<IndexKey>) -> RangeIterImpl<'_> {
        match self {
            IdxImpl::Bool(idx) => {
                let start = match range.start_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_bool().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_bool().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                let end = match range.end_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_bool().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_bool().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                RangeIterImpl::Bool(idx.values_in_range(&(start, end)))
            },
            IdxImpl::U8(idx) => {
                let start = match range.start_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_u8().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_u8().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                let end = match range.end_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_u8().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_u8().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                RangeIterImpl::U8(idx.values_in_range(&(start, end)))
            },
            IdxImpl::U16(idx) => {
                let start = match range.start_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_u16().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_u16().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                let end = match range.end_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_u16().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_u16().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                RangeIterImpl::U16(idx.values_in_range(&(start, end)))
            },
            IdxImpl::U32(idx) => {
                let start = match range.start_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_u32().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_u32().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                let end = match range.end_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_u32().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_u32().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                RangeIterImpl::U32(idx.values_in_range(&(start, end)))
            },
            IdxImpl::U64(idx) => {
                let start = match range.start_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_u64().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_u64().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                let end = match range.end_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_u64().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_u64().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                RangeIterImpl::U64(idx.values_in_range(&(start, end)))
            },
            IdxImpl::U128(idx) => {
                let start = match range.start_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_u128().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_u128().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                let end = match range.end_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_u128().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_u128().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                RangeIterImpl::U128(idx.values_in_range(&(start, end)))
            },
            IdxImpl::I8(idx) => {
                let start = match range.start_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_i8().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_i8().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                let end = match range.end_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_i8().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_i8().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                RangeIterImpl::I8(idx.values_in_range(&(start, end)))
            },
            IdxImpl::I16(idx) => {
                let start = match range.start_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_i16().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_i16().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                let end = match range.end_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_i16().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_i16().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                RangeIterImpl::I16(idx.values_in_range(&(start, end)))
            },
            IdxImpl::I32(idx) => {
                let start = match range.start_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_i32().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_i32().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                let end = match range.end_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_i32().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_i32().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                RangeIterImpl::I32(idx.values_in_range(&(start, end)))
            },
            IdxImpl::I64(idx) => {
                let start = match range.start_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_i64().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_i64().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                let end = match range.end_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_i64().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_i64().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                RangeIterImpl::I64(idx.values_in_range(&(start, end)))
            },
            IdxImpl::I128(idx) => {
                let start = match range.start_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_i128().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_i128().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                let end = match range.end_bound() {
                    std::ops::Bound::Included(v) => std::ops::Bound::Included(*v.as_i128().unwrap()),
                    std::ops::Bound::Excluded(v) => std::ops::Bound::Excluded(*v.as_i128().unwrap()),
                    std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
                };
                RangeIterImpl::I128(idx.values_in_range(&(start, end)))
            },
            IdxImpl::AlgebraicValue(idx) => {
                RangeIterImpl::AlgebraicValue(idx.values_in_range(range))
            },
        }
    }

    pub fn clear(&mut self) {
        match self {
            IdxImpl::Bool(idx) => idx.clear(),
            IdxImpl::U8(idx) => idx.clear(),
            IdxImpl::U16(idx) => idx.clear(),
            IdxImpl::U32(idx) => idx.clear(),
            IdxImpl::U64(idx) => idx.clear(),
            IdxImpl::U128(idx) => idx.clear(),
            IdxImpl::I8(idx) => idx.clear(),
            IdxImpl::I16(idx) => idx.clear(),
            IdxImpl::I32(idx) => idx.clear(),
            IdxImpl::I64(idx) => idx.clear(),
            IdxImpl::I128(idx) => idx.clear(),
            IdxImpl::AlgebraicValue(idx) => idx.clear(),
        }
    }

}

static_assert_size!(BTreeIndex, 56);

impl BTreeIndex {
    /// Returns a new possibly unique index, with `index_id` for a set of columns.
    /// TODO: cols should really be stored in the BTreeIndex to avoid consistency issues. Right now we're always passing it in with every call
    /// I would fix it now, but I don't want to blow up the diff
    pub fn new(index_id: IndexId, row_type: &ProductType, cols: &ColList, is_unique: bool, name: impl Into<Box<str>>) -> Self {
        let idx_impl = if cols.len() == 1 {
            let col_pos = cols.head().idx();
            let element = row_type.elements.get(col_pos).ok_or(InvalidFieldError {
                col_pos: col_pos.into(),
                name: None,
            }).unwrap();
            match element.algebraic_type {
                spacetimedb_sats::AlgebraicType::Builtin(BuiltinType::Bool) => IdxImpl::Bool(MultiMap::<bool, RowPointer>::new()),
                spacetimedb_sats::AlgebraicType::Builtin(BuiltinType::U8) => IdxImpl::U8(MultiMap::<u8, RowPointer>::new()),
                spacetimedb_sats::AlgebraicType::Builtin(BuiltinType::U16) => IdxImpl::U16(MultiMap::<u16, RowPointer>::new()),
                spacetimedb_sats::AlgebraicType::Builtin(BuiltinType::U32) => IdxImpl::U32(MultiMap::<u32, RowPointer>::new()),
                spacetimedb_sats::AlgebraicType::Builtin(BuiltinType::U64) => IdxImpl::U64(MultiMap::<u64, RowPointer>::new()),
                spacetimedb_sats::AlgebraicType::Builtin(BuiltinType::U128) => IdxImpl::U128(MultiMap::<u128, RowPointer>::new()),
                spacetimedb_sats::AlgebraicType::Builtin(BuiltinType::I8) => IdxImpl::I8(MultiMap::<i8, RowPointer>::new()),
                spacetimedb_sats::AlgebraicType::Builtin(BuiltinType::I16) => IdxImpl::I16(MultiMap::<i16, RowPointer>::new()),
                spacetimedb_sats::AlgebraicType::Builtin(BuiltinType::I32) => IdxImpl::I32(MultiMap::<i32, RowPointer>::new()),
                spacetimedb_sats::AlgebraicType::Builtin(BuiltinType::I64) => IdxImpl::I64(MultiMap::<i64, RowPointer>::new()),
                spacetimedb_sats::AlgebraicType::Builtin(BuiltinType::I128) => IdxImpl::I128(MultiMap::<i128, RowPointer>::new()),
                _ => {
                    IdxImpl::AlgebraicValue(MultiMap::<IndexKey, RowPointer>::new())
                }
            }
        } else {
            IdxImpl::AlgebraicValue(MultiMap::<IndexKey, RowPointer>::new())
        };
        Self {
            index_id,
            is_unique,
            idx: idx_impl,
            name: name.into(),
        }
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
        let col_value = self.get_fields(cols, row)?;
        self.idx.insert(col_value, ptr)
    }

    /// Deletes `ptr` with its indexed value `col_value` from this index.
    ///
    /// Returns whether `ptr` was present.
    pub fn delete(&mut self, col_value: &AlgebraicValue, ptr: RowPointer) -> bool {
        self.idx.delete(col_value, &ptr)
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
    use spacetimedb_sats::product;

    fn gen_row_pointer() -> impl Strategy<Value = RowPointer> {
        (any::<PageOffset>(), any::<PageIndex>()).prop_map(|(po, pi)| RowPointer::new(false, pi, po, SquashedOffset(0)))
    }

    fn gen_cols(ty_len: usize) -> impl Strategy<Value = ColList> {
        vec((0..ty_len as u32).prop_map_into(), 1..=ty_len)
            .prop_map(|cols| cols.into_iter().collect::<ColListBuilder>().build().unwrap())
    }

    fn gen_row_and_cols() -> impl Strategy<Value = (ColList, ProductValue)> {
        generate_row_type(1..16).prop_flat_map(|ty| (ty, gen_cols(ty.elements.len()), generate_product_value(ty)))
    }

    fn new_index(row_type: &ProductType, cols: &ColList, is_unique: bool) -> BTreeIndex {
        BTreeIndex::new(0.into(), row_type, cols, is_unique, "test_index")
    }

    proptest! {
        #[test]
        fn remove_nonexistent_noop(((ty, cols, pv), ptr, is_unique) in (gen_row_and_cols(), gen_row_pointer(), any::<bool>())) {
            let mut index = new_index(ty, cols, is_unique);
            let value = index.get_fields(&cols, &pv).unwrap();
            prop_assert_eq!(index.delete(&value, ptr), false);
            prop_assert!(index.idx.is_empty());
        }

        #[test]
        fn insert_delete_noop(((ty, cols, pv), ptr, is_unique) in (gen_row_and_cols(), gen_row_pointer(), any::<bool>())) {
            let mut index = new_index(ty, cols, is_unique);
            let value = index.get_fields(&cols, &pv).unwrap();
            prop_assert_eq!(index.idx.len(), 0);
            prop_assert_eq!(index.contains_any(&value), false);

            prop_assert_eq!(index.insert(&cols, &pv, ptr).unwrap(), true);
            prop_assert_eq!(index.idx.len(), 1);
            prop_assert_eq!(index.contains_any(&value), true);

            // Try inserting again, it should fail.
            prop_assert_eq!(index.insert(&cols, &pv, ptr).unwrap(), false);
            prop_assert_eq!(index.idx.len(), 1);

            prop_assert_eq!(index.delete(&value, ptr), true);
            prop_assert_eq!(index.idx.len(), 0);
            prop_assert_eq!(index.contains_any(&value), false);
        }

        #[test]
        fn insert_again_violates_unique_constraint(((ty, cols, pv), ptr) in (gen_row_and_cols(), gen_row_pointer())) {
            let mut index = new_index(ty, cols, true);
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
            let mut index = new_index(product_type![AlgebraicType::U64], cols, true);

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
