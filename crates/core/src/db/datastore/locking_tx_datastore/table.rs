use super::{
    btree_index::{BTreeIndex, BTreeIndexRangeIter},
    hash_index::HashIndex,
    RowId,
};
use crate::{db::datastore::traits::TableSchema, error::DBError};
use indexmap::IndexMap;
use nonempty::NonEmpty;
use spacetimedb_primitives::ColId;
use spacetimedb_sats::{AlgebraicValue, ProductType, ProductValue};
use std::{collections::HashMap, ops::RangeBounds};

#[derive(Default)]
pub(crate) struct ColIndexes {
    btree: Option<BTreeIndex>,
    hash: Option<HashIndex>,
}

impl ColIndexes {
    pub(crate) fn insert(&mut self, row: &ProductValue) -> Result<(), DBError> {
        if let Some(btree) = &mut self.btree {
            btree.insert(row)?;
        }
        if let Some(hash) = &mut self.hash {
            hash.insert(row)?;
        }
        Ok(())
    }

    pub(crate) fn delete(&mut self, col_value: &AlgebraicValue, row_id: &RowId) {
        if let Some(btree) = &mut self.btree {
            btree.delete(col_value, row_id);
        }
        if let Some(hash) = &mut self.hash {
            hash.delete(col_value, row_id);
        }
    }

    pub(crate) fn seek(&self, range: &impl RangeBounds<AlgebraicValue>) -> BTreeIndexRangeIter<'_> {
        self.btree.as_ref().unwrap().seek(range)
    }
}

pub(crate) struct Table {
    pub(crate) row_type: ProductType,
    pub(crate) schema: TableSchema,
    pub(crate) indexes: HashMap<NonEmpty<ColId>, ColIndexes>,
    pub(crate) rows: IndexMap<RowId, ProductValue>,
}

impl Table {
    pub(crate) fn new(row_type: ProductType, schema: TableSchema) -> Self {
        Self {
            row_type,
            schema,
            indexes: Default::default(),
            rows: Default::default(),
        }
    }

    pub(crate) fn insert_btree_index(&mut self, mut index: BTreeIndex) {
        index.build_from_rows(self.scan_rows()).unwrap();
        self.indexes.entry(index.cols.clone()).or_default().btree = Some(index);
    }

    pub(crate) fn insert_hash_index(&mut self, mut index: HashIndex) {
        index.build_from_rows(self.scan_rows()).unwrap();
        self.indexes.entry(index.cols.clone()).or_default().hash = Some(index);
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
