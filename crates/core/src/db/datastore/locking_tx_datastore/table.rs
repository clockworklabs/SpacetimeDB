use super::{
    btree_index::{BTreeIndex, BTreeIndexIter, BTreeIndexRangeIter},
    RowId,
};
use crate::db::datastore::traits::{ColId, TableSchema};
use nonempty::NonEmpty;
use spacetimedb_sats::{AlgebraicValue, ProductType, ProductValue};
use std::{
    collections::{BTreeMap, HashMap},
    ops::RangeBounds,
};

pub(crate) struct Table {
    pub(crate) row_type: ProductType,
    pub(crate) schema: TableSchema,
    pub(crate) indexes: HashMap<NonEmpty<ColId>, BTreeIndex>,
    pub(crate) rows: BTreeMap<RowId, ProductValue>,
}

impl Table {
    pub(crate) fn insert_index(&mut self, mut index: BTreeIndex) {
        index.build_from_rows(self.scan_rows()).unwrap();
        self.indexes.insert(index.cols.clone().map(ColId), index);
    }

    pub(crate) fn insert(&mut self, row_id: RowId, row: ProductValue) {
        for (_, index) in self.indexes.iter_mut() {
            index.insert(&row).unwrap();
        }
        self.rows.insert(row_id, row);
    }

    pub(crate) fn delete(&mut self, row_id: &RowId) -> Option<ProductValue> {
        let row = self.rows.remove(row_id)?;
        for (cols, index) in self.indexes.iter_mut() {
            let col_value = row.project_not_empty(&cols.clone().map(|x| x.0)).unwrap();
            index.delete(&col_value, row_id)
        }
        Some(row)
    }

    pub(crate) fn get_row(&self, row_id: &RowId) -> Option<&ProductValue> {
        self.rows.get(row_id)
    }

    pub(crate) fn get_row_type(&self) -> &ProductType {
        &self.row_type
    }

    pub(crate) fn get_schema(&self) -> &TableSchema {
        &self.schema
    }

    pub(crate) fn scan_rows(&self) -> impl Iterator<Item = &ProductValue> {
        self.rows.values()
    }

    /// When there's an index for `cols`,
    /// returns an iterator over the [`BTreeIndex`] that yields all the `RowId`s
    /// that match the specified `value` in the indexed column.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    ///
    /// For a unique index this will always yield at most one `RowId`.
    pub(crate) fn index_seek<'a>(
        &'a self,
        cols: NonEmpty<ColId>,
        value: &'a AlgebraicValue,
    ) -> Option<BTreeIndexRangeIter<'a>> {
        self.indexes.get(&cols).map(|index| index.seek(value))
    }

    pub(crate) fn _index_scan(&self, cols: NonEmpty<ColId>) -> BTreeIndexIter<'_> {
        self.indexes.get(&cols).unwrap().scan()
    }

    pub(crate) fn _index_range_scan(
        &self,
        cols: NonEmpty<ColId>,
        range: impl RangeBounds<AlgebraicValue>,
    ) -> BTreeIndexRangeIter<'_> {
        self.indexes.get(&cols).unwrap().scan_range(range)
    }
}
