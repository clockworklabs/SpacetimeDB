use super::{
    btree_index::{BTreeIndex, BTreeIndexRangeIter},
    RowId,
};
use crate::db::datastore::traits::{ColId, TableSchema};
use spacetimedb_sats::{AlgebraicValue, ProductType, ProductValue};
use std::{
    collections::{BTreeMap, HashMap},
    ops::RangeBounds,
};

pub(crate) struct Table {
    pub(crate) row_type: ProductType,
    pub(crate) schema: TableSchema,
    pub(crate) indexes: HashMap<ColId, BTreeIndex>,
    pub(crate) rows: BTreeMap<RowId, ProductValue>,
}

impl Table {
    pub(crate) fn insert_index(&mut self, mut index: BTreeIndex) {
        index.build_from_rows(self.scan_rows()).unwrap();
        self.indexes.insert(ColId(index.col_id), index);
    }

    pub(crate) fn insert(&mut self, row_id: RowId, row: ProductValue) {
        for (_, index) in self.indexes.iter_mut() {
            index.insert(&row).unwrap();
        }
        self.rows.insert(row_id, row);
    }

    pub(crate) fn delete(&mut self, row_id: &RowId) -> Option<ProductValue> {
        let row = self.rows.remove(row_id)?;
        for (col_id, index) in self.indexes.iter_mut() {
            let col_value = row.get_field(col_id.0 as usize, None).unwrap();
            index.delete(col_value, row_id)
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

    /// When there's an index for `col_id`,
    /// returns an iterator over the [`BTreeIndex`] that yields all the `RowId`s
    /// that match the specified `range` in the indexed column.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub(crate) fn index_seek(
        &self,
        col_id: ColId,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<BTreeIndexRangeIter<'_>> {
        self.indexes.get(&col_id).map(|index| index.seek(range))
    }
}
