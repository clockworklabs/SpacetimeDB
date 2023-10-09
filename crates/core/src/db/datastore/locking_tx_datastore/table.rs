use super::{
    btree_index::{BTreeIndex, BTreeIndexRangeIter},
    RowId,
};
use crate::db::datastore::traits::{ColId, TableSchema};
use nonempty::NonEmpty;
use spacetimedb_sats::{AlgebraicValue, ProductType, ProductValue};
use std::{
    collections::{BTreeMap, HashMap},
    ops::RangeBounds,
};
//
// fn set_true(val: &mut usize, bit_pos: usize) {
//     *val |= 1 << bit_pos;
// }
//
// fn set_false(val: &mut usize, bit_pos: usize) {
//     *val &= !(1 << bit_pos);
// }
//
// fn is_set(val: usize, bit_pos: usize) -> bool {
//     let mask = 1 << bit_pos;
//     val & mask == 1
// }

#[derive(Debug, Clone)]
pub struct RowPk {
    pub(crate) key: RowId,
    pub(crate) row: ProductValue,
}

#[derive(Debug, Clone)]
pub struct Rows {
    data: Vec<RowPk>,
    index: BTreeMap<RowId, usize>,
}

impl Default for Rows {
    fn default() -> Self {
        Self {
            data: Vec::new(),
            index: BTreeMap::new(),
        }
    }
}

impl Rows {
    pub fn insert(&mut self, key: RowId, value: ProductValue) -> Option<&ProductValue> {
        let pos = if let Some(pos) = self.index.get(&key) {
            *pos
        } else {
            let idx = self.data.len();
            self.index.insert(key, idx);
            self.data.push(RowPk { key, row: value });
            idx
        };
        // set_false(&mut self.deleted, pos);

        self.data.get(pos).map(|x| &x.row)
    }

    pub fn remove(&mut self, key: &RowId) -> Option<ProductValue> {
        if let Some(pos) = self.index.remove(key) {
            // set_true(&mut self.deleted, pos);

            self.data.get(pos).map(|x| x.row.clone())
        } else {
            None
        }
    }

    pub fn get(&self, key: &RowId) -> Option<&ProductValue> {
        if let Some(pos) = self.index.get(key) {
            self.data.get(*pos).map(|x| &x.row)
        } else {
            None
        }
    }

    #[inline]
    pub fn iter(&self) -> RowsIterator<'_> {
        RowsIterator::new(self)
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }
}

pub struct RowsIterator<'a> {
    rows: &'a Rows,
    iter: std::slice::Iter<'a, RowPk>,
}

//unsafe impl<'a> std::iter::TrustedLen for RowsIterator<'a> {}

impl<'a> ExactSizeIterator for RowsIterator<'a> {}

impl<'a> RowsIterator<'a> {
    pub fn new(rows: &'a Rows) -> Self {
        RowsIterator {
            rows,
            iter: rows.data.iter(),
        }
    }
}

impl<'a> Iterator for RowsIterator<'a> {
    type Item = &'a RowPk;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(row) = self.iter.next() {
            //if self.rows.index.contains_key(&row.key) {
            return Some(row);
            //}
        };
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.rows.len(), Some(self.rows.len()))
    }
}

impl<'a> IntoIterator for &'a Rows {
    type Item = &'a RowPk;
    type IntoIter = RowsIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        RowsIterator::new(self)
    }
}

pub(crate) struct Table {
    pub(crate) row_type: ProductType,
    pub(crate) schema: TableSchema,
    pub(crate) indexes: HashMap<NonEmpty<ColId>, BTreeIndex>,
    pub(crate) rows: Rows,
    //    pub(crate) rows: BTreeMap<RowId, ProductValue>,
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
        self.rows.iter().map(|x| &x.row)
    }

    /// When there's an index for `cols`,
    /// returns an iterator over the [`BTreeIndex`] that yields all the `RowId`s
    /// that match the specified `range` in the indexed column.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub(crate) fn index_seek(
        &self,
        cols: NonEmpty<ColId>,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<BTreeIndexRangeIter<'_>> {
        self.indexes.get(&cols).map(|index| index.seek(range))
    }
}
