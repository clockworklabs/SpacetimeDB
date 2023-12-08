use super::{
    btree_index::{BTreeIndex, BTreeIndexRangeIter},
    RowId,
};
use indexmap::IndexMap;
use nonempty::NonEmpty;
use spacetimedb_primitives::ColId;
use spacetimedb_sats::db::def::TableSchema;
use spacetimedb_sats::{AlgebraicValue, ProductType, ProductValue};
use std::{collections::HashMap, ops::RangeBounds};

pub(crate) struct Table {
    pub(crate) schema: TableSchema,
    pub(crate) indexes: HashMap<NonEmpty<ColId>, BTreeIndex>,
    pub(crate) rows: IndexMap<RowId, ProductValue>,
}

impl Table {
    pub(crate) fn new(schema: TableSchema) -> Self {
        Self {
            schema,
            indexes: Default::default(),
            rows: Default::default(),
        }
    }

    pub(crate) fn insert_index(&mut self, mut index: BTreeIndex) {
        index.build_from_rows(self.scan_rows()).unwrap();
        self.indexes.insert(index.cols.clone(), index);
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
            let col_value = row.project_not_empty(cols).unwrap();
            index.delete(&col_value, row_id)
        }
        Some(row)
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn get_row(&self, row_id: &RowId) -> Option<&ProductValue> {
        self.rows.get(row_id)
    }

    pub(crate) fn get_row_type(&self) -> &ProductType {
        self.schema.get_row_type()
    }

    pub(crate) fn get_schema(&self) -> &TableSchema {
        &self.schema
    }

    pub(crate) fn scan_rows(&self) -> impl Iterator<Item = &ProductValue> {
        self.rows.values()
    }

    /// When there's an index for `cols`,
    /// returns an iterator over the [`BTreeIndex`] that yields all the `RowId`s
    /// that match the specified `range` in the indexed column.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub(crate) fn index_seek(
        &self,
        cols: &NonEmpty<ColId>,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<BTreeIndexRangeIter<'_>> {
        self.indexes.get(cols).map(|index| index.seek(range))
    }
}
