use std::collections::btree_map::{Iter, Range};
use std::collections::{BTreeMap, Bound};
use std::fmt;
use std::ops::RangeBounds;

use crate::db::index::btree;
use crate::db::relational_db::{RelationalDB, TableIter};
use crate::db::transactional_db::Tx;
use crate::error::{DBError, IndexError};
use spacetimedb_lib::data_key::ToDataKey;
use spacetimedb_lib::{DataKey, PrimaryKey, TupleDef, TupleValue, TypeDef, TypeValue};

/// The `id` for [crate::db::sequence::Sequence]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IndexId(pub(crate) u32);

impl fmt::Display for IndexId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<usize> for IndexId {
    fn from(x: usize) -> Self {
        IndexId(x as u32)
    }
}

impl From<i64> for IndexId {
    fn from(x: i64) -> Self {
        IndexId(x as u32)
    }
}

impl From<i32> for IndexId {
    fn from(x: i32) -> Self {
        IndexId(x as u32)
    }
}

impl From<u32> for IndexId {
    fn from(x: u32) -> Self {
        IndexId(x)
    }
}

#[derive(Debug)]
pub enum IndexFields {
    IndexId = 0,
    TableId,
    ColId,
    IndexName,
    IsUnique,
}

impl IndexFields {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            IndexFields::IndexId => "index_id",
            IndexFields::TableId => "table_id",
            IndexFields::ColId => "col_id",
            IndexFields::IndexName => "index_name",
            IndexFields::IsUnique => "is_unique",
        }
    }
}

impl From<IndexFields> for Option<&'static str> {
    fn from(x: IndexFields) -> Self {
        Some(x.name())
    }
}

impl From<IndexFields> for Option<String> {
    fn from(x: IndexFields) -> Self {
        Some(x.name().into())
    }
}

/// The key for the [BTreeIndex].
///
/// It stores a pair of ([TypeValue]/[DataKey]) as a key for the [BTreeIndex].
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct IndexKey {
    value: TypeValue,
    key: DataKey,
}

impl IndexKey {
    pub(crate) fn from_value(value: &TypeValue) -> Self {
        let key = value.to_data_key();
        Self {
            value: value.clone(),
            key,
        }
    }

    pub(crate) fn from_row(value: &TypeValue, data: DataKey) -> Self {
        Self {
            value: value.clone(),
            key: data,
        }
    }
}

/// Define the properties for build an [BTreeIndex]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct IndexDef {
    pub(crate) table_id: u32,
    pub(crate) col_id: u32,
    pub(crate) is_unique: bool,
    pub(crate) name: String,
}

impl IndexDef {
    /// WARNING: Assumes `table_id`, `col_id` are valid
    pub fn new(name: &str, table_id: u32, col_id: u32, is_unique: bool) -> Self {
        Self {
            name: name.to_string(),
            table_id,
            col_id,
            is_unique,
        }
    }
}

impl fmt::Display for IndexDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "`{}` {} (table_id:{}/col_id:{})",
            &self.name,
            if self.is_unique { "UNIQUE" } else { "NOT UNIQUE" },
            self.table_id,
            self.col_id
        )
    }
}

/// A [BTreeMap] backed indexing structure that allows both UNIQUE/NOT UNIQUE rows
#[derive(Debug, Clone)]
pub struct BTreeIndex {
    pub(crate) index_id: IndexId,
    pub(crate) table_id: u32,
    pub(crate) col_id: u32,
    pub(crate) is_unique: bool,
    pub(crate) name: String,
    pub(crate) idx: BTreeMap<IndexKey, DataKey>,
}

impl BTreeIndex {
    /// WARNING: Assumes `index_id`, `table_id`, `col_id` are valid
    pub fn new(name: &str, index_id: IndexId, table_id: u32, col_id: u32, is_unique: bool) -> Self {
        Self {
            index_id,
            idx: BTreeMap::new(),
            name: name.to_string(),
            table_id,
            col_id,
            is_unique,
        }
    }

    /// Create an [BTreeIndex] from an [IndexDef]
    ///
    /// WARNING: Assumes the `index_id` is valid
    pub fn from_def(index_id: IndexId, of: IndexDef) -> Self {
        Self::new(&of.name, index_id, of.table_id, of.col_id, of.is_unique)
    }

    /// Returns the number of all rows in the [BTreeIndex].
    pub fn len(&self) -> usize {
        self.idx.len()
    }

    pub fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }

    /// Clears the [BTreeIndex], removing all rows.
    pub fn clear(&mut self) {
        self.idx.clear();
    }

    pub(crate) fn get_key<'a>(&'a self, row: &'a TupleValue) -> Result<&'a TypeValue, IndexError> {
        if let Some(key) = row.elements.get(self.col_id as usize) {
            Ok(key)
        } else {
            Err(IndexError::ColumnNotFound(self.into()))
        }
    }

    fn insert_unique(&mut self, row: &TupleValue) -> Result<(), DBError> {
        assert!(self.is_unique, "This is valid only for UNIQUE indexes");
        let key = IndexKey::from_value(self.get_key(row)?);
        self.idx.insert(key, row.to_data_key());
        Ok(())
    }

    fn insert_duplicate(&mut self, row: &TupleValue) -> Result<(), DBError> {
        assert!(!self.is_unique, "This is valid only for NOT UNIQUE indexes");
        let key = self.get_key(row)?;

        let key = IndexKey::from_row(key, row.to_data_key());
        self.idx.insert(key, row.to_data_key());

        Ok(())
    }

    /// Fill the [BTreeIndex] from a [TupleValue] and dispatch to the correct logic for UNIQUE/NOT UNIQUE
    pub fn index_row(&mut self, row: &TupleValue) -> Result<(), DBError> {
        if self.is_unique {
            self.insert_unique(row)?;
        } else {
            self.insert_duplicate(row)?;
        }

        Ok(())
    }

    /// Fill the [BTreeIndex] from a [Iterator] and dispatch to the correct logic for UNIQUE/NOT UNIQUE
    pub fn index_rows(&mut self, rows: impl Iterator<Item = TupleValue>) -> Result<(), DBError> {
        if self.is_unique {
            for row in rows {
                self.insert_unique(&row)?;
            }
        } else {
            for row in rows {
                self.insert_duplicate(&row)?;
            }
        }

        Ok(())
    }

    /// Fill the [BTreeIndex] from a Scan of the full column
    pub fn index_full_column(&mut self, stdb: &RelationalDB, tx: &mut Tx) -> Result<(), DBError> {
        self.clear();
        let rows = stdb.scan(tx, self.table_id)?;
        self.index_rows(rows)?;
        Ok(())
    }

    /// Returns `true` if the [BTreeIndex] contains a value for the specified `key`.
    pub fn contains_key(&self, key: &TypeValue) -> bool {
        if self.is_unique {
            self.idx.contains_key(&IndexKey::from_value(key))
        } else {
            let k = IndexKey::from_value(key);
            self.idx.range(k..).next().is_some()
        }
    }

    /// Returns a [Iterator] for the rows that match the key.
    ///
    /// NOTE: It could return many rows per key if the [BTreeIndex] allows duplicates
    /// the rows are returned by insertion order in that case.
    pub fn get<'a>(&'a self, stdb: &'a RelationalDB, tx: &'a mut Tx, key: &'a TypeValue) -> ValuesIter<'a> {
        let k = IndexKey::from_value(key);
        ValuesIter {
            stdb,
            tx,
            table_id: self.table_id,
            key,
            idx: self.idx.range(k..),
        }
    }

    /// Returns a [Iterator] for the rows.
    ///
    /// NOTE: It could return many rows per key if the [BTreeIndex] allows duplicates
    /// the rows are returned by [DataKey] order in that case.
    pub fn iter<'a>(&'a self, stdb: &'a RelationalDB, tx: &'a mut Tx) -> TuplesIter<'a> {
        TuplesIter {
            stdb,
            tx,
            table_id: self.table_id,
            idx: self.idx.iter(),
        }
    }

    pub fn iter_range<'a, R>(&'a self, stdb: &'a RelationalDB, tx: &'a mut Tx, range: R) -> TuplesRangeIter<'a>
    where
        R: RangeBounds<TypeValue>,
    {
        let start = match range.start_bound() {
            Bound::Included(x) => Bound::Included(IndexKey::from_value(x)),
            Bound::Excluded(x) => Bound::Excluded(IndexKey::from_value(x)),
            Bound::Unbounded => Bound::Unbounded,
        };

        let end = match range.end_bound() {
            Bound::Included(x) => Bound::Included(IndexKey::from_value(x)),
            Bound::Excluded(x) => Bound::Excluded(IndexKey::from_value(x)),
            Bound::Unbounded => Bound::Unbounded,
        };
        TuplesRangeIter {
            stdb,
            tx,
            table_id: self.table_id,
            idx: self.idx.range((start, end)),
        }
    }
}

impl From<&BTreeIndex> for IndexDef {
    fn from(x: &BTreeIndex) -> Self {
        IndexDef {
            table_id: x.table_id,
            col_id: x.col_id,
            is_unique: x.is_unique,
            name: x.name.clone(),
        }
    }
}

impl From<&mut BTreeIndex> for IndexDef {
    fn from(x: &mut BTreeIndex) -> Self {
        IndexDef {
            table_id: x.table_id,
            col_id: x.col_id,
            is_unique: x.is_unique,
            name: x.name.clone(),
        }
    }
}

fn _unpack_tuple(tuple: Result<Option<TupleValue>, DBError>) -> Option<TupleValue> {
    match tuple {
        Ok(x) => x,
        Err(err) => {
            panic!("Failed iterating tuples: {err}");
        }
    }
}

/// An iterator for the rows that match a key [TypeValue] on the [BTreeIndex]
pub struct ValuesIter<'a> {
    stdb: &'a RelationalDB,
    tx: &'a mut Tx,
    table_id: u32,
    key: &'a TypeValue,
    idx: btree::Range<'a, IndexKey, DataKey>,
}

impl<'a> Iterator for ValuesIter<'a> {
    type Item = TupleValue;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((k, data_key)) = self.idx.next() {
            if &k.value != self.key {
                return None;
            }

            return _unpack_tuple(
                self.stdb
                    .pk_seek(self.tx, self.table_id, PrimaryKey { data_key: *data_key }),
            );
        }
        None
    }
}

/// An iterator by [IndexKey] for the rows of the [BTreeIndex]
pub struct TuplesIter<'a> {
    stdb: &'a RelationalDB,
    tx: &'a mut Tx,
    table_id: u32,
    idx: Iter<'a, IndexKey, DataKey>,
}

impl<'a> Iterator for TuplesIter<'a> {
    type Item = TupleValue;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((_key, data_key)) = self.idx.next() {
            return _unpack_tuple(
                self.stdb
                    .pk_seek(self.tx, self.table_id, PrimaryKey { data_key: *data_key }),
            );
        }
        None
    }
}

/// An iterator for a range of [TypeValue] on the [BTreeIndex]
pub struct TuplesRangeIter<'a> {
    stdb: &'a RelationalDB,
    tx: &'a mut Tx,
    table_id: u32,
    idx: Range<'a, IndexKey, DataKey>,
}

impl<'a> Iterator for TuplesRangeIter<'a> {
    type Item = TupleValue;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((_key, data_key)) = self.idx.next() {
            return _unpack_tuple(
                self.stdb
                    .pk_seek(self.tx, self.table_id, PrimaryKey { data_key: *data_key }),
            );
        }
        None
    }
}

/// Table [ST_INDEXES_NAME]
///
/// | index_id: u32 | table_id: u32 | col_id: u32 | index_name: String | is_unique: bool |
/// |---------------|---------------|-------------|--------------------|-----------------|
/// | 1             | 1             | 1           | ix_sample          | true            |
pub(crate) fn internal_schema() -> TupleDef {
    TupleDef::from_iter([
        (IndexFields::IndexId.name(), TypeDef::U32),
        (IndexFields::TableId.name(), TypeDef::U32),
        (IndexFields::ColId.name(), TypeDef::U32),
        (IndexFields::IndexName.name(), TypeDef::String),
        (IndexFields::IsUnique.name(), TypeDef::Bool),
    ])
}

pub fn decode_schema(row: TupleValue) -> Result<BTreeIndex, DBError> {
    let index_id = row.field_as_u32(IndexFields::IndexId as usize, IndexFields::IndexId.into())?;
    let index_name = row.field_as_str(IndexFields::IndexName as usize, IndexFields::IndexName.into())?;
    let table_id = row.field_as_u32(IndexFields::TableId as usize, IndexFields::TableId.into())?;
    let col_id = row.field_as_u32(IndexFields::ColId as usize, IndexFields::ColId.into())?;
    let is_unique = row.field_as_bool(IndexFields::IsUnique as usize, IndexFields::IsUnique.into())?;

    let index = IndexDef::new(index_name, table_id, col_id, is_unique);

    Ok(BTreeIndex::from_def(IndexId(index_id), index))
}

pub struct IndexIter<'a> {
    pub(crate) table_iter: TableIter<'a>,
}

impl<'a> Iterator for IndexIter<'a> {
    type Item = BTreeIndex;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(row) = self.table_iter.next() {
            return decode_schema(row).ok();
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::db::relational_db::RelationalDB;
    use crate::db::transactional_db::TxCtx;
    use crate::error::IndexError;
    use spacetimedb_lib::error::{ResultTest, TestError};
    use spacetimedb_lib::TypeValue;
    use spacetimedb_sats::{product, BuiltinValue};
    use std::ops::Range;

    fn _create_data(stdb: &mut RelationalDB, range: Range<i32>) -> ResultTest<u32> {
        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let table_id = stdb.create_table(
            tx,
            "MyTable",
            TupleDef::from_iter([("my_i32", TypeDef::I32), ("my_txt", TypeDef::String)]),
        )?;

        for x in range {
            let txt = match x % 3 {
                0 => 'A',
                1 => 'B',
                _ => 'C',
            };

            stdb.insert(
                tx,
                table_id,
                product![BuiltinValue::I32(x), BuiltinValue::String(txt.into())],
            )?;
        }
        tx_.commit()?;

        Ok(table_id)
    }

    fn _find(idx: &BTreeIndex, key: &TypeValue, stdb: &RelationalDB, tx: &mut Tx) -> ResultTest<Vec<Vec<TypeValue>>> {
        let mut result = Vec::new();

        for row in idx.get(stdb, tx, &key) {
            result.push(row.elements.to_vec());
        }

        Ok(result)
    }

    #[test]
    fn test_index_create() -> ResultTest<()> {
        let (mut stdb, _tmp_dir) = make_test_db()?;
        let table_id = _create_data(&mut stdb, 0..9)?;
        let idx = IndexDef::new("idx_1", table_id, 0, true);

        stdb.begin_tx().with(|tx, stdb| {
            let index_id = stdb.create_index(tx, idx)?;

            let idx = stdb.catalog.indexes.get("idx_1");
            assert!(idx.is_some(), "Index is not found");

            assert!(stdb.drop_index(tx, index_id).is_ok());
            stdb.commit_tx(tx.clone())
        })?;

        let idx = stdb.catalog.indexes.get("idx_1");
        assert!(idx.is_none(), "Index still exist");

        Ok(())
    }

    #[test]
    fn test_index_unique() -> ResultTest<()> {
        let (mut stdb, _tmp_dir) = make_test_db()?;
        let table_id = _create_data(&mut stdb, 0..9)?;

        stdb.begin_tx().with(|tx, stdb| {
            let idx = IndexDef::new("idx_1", table_id, 0, true);
            let mut idx = BTreeIndex::from_def(0.into(), idx);
            idx.index_full_column(&stdb, tx)?;

            assert_eq!(idx.len(), 9, "Wrong number of index unique keys");

            let key = TypeValue::I32(1);
            let values = _find(&idx, &key, &stdb, tx)?;
            assert_eq!(values, vec![vec![TypeValue::I32(1), TypeValue::String("B".into())]]);

            Ok::<(), TestError>(())
        })?;

        Ok(())
    }

    #[test]
    fn test_index_not_uniques() -> ResultTest<()> {
        let (mut stdb, _tmp_dir) = make_test_db()?;
        let table_id = _create_data(&mut stdb, 0..9)?;

        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let idx = IndexDef::new("idx_1", table_id, 1, false);
        let mut idx = BTreeIndex::from_def(0.into(), idx);
        idx.index_full_column(&stdb, tx)?;

        let key = TypeValue::String("A".into());
        let mut values = _find(&idx, &key, &stdb, tx)?;
        values.sort();
        assert_eq!(
            values,
            vec![
                vec![TypeValue::I32(0), TypeValue::String("A".into())],
                vec![TypeValue::I32(3), TypeValue::String("A".into())],
                vec![TypeValue::I32(6), TypeValue::String("A".into())]
            ]
        );

        Ok(())
    }

    #[test]
    fn test_index_duplicate() -> ResultTest<()> {
        let (mut stdb, _tmp_dir) = make_test_db()?;
        let table_id = _create_data(&mut stdb, 0..9)?;

        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let idx = IndexDef::new("idx_1", table_id, 0, true);
        stdb.create_index(tx, idx)?;

        stdb.commit_tx(tx.clone())?;

        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        match stdb.insert(
            tx,
            table_id,
            product![TypeValue::I32(0), TypeValue::String("Row dup".into())],
        ) {
            Ok(_) => {
                panic!("Insert duplicated row")
            }
            Err(e) => match e {
                DBError::Index(IndexError::Duplicated(_, _, _)) => {}
                err => panic!("Error with duplicated row: {err}"),
            },
        }
        Ok(())
    }

    #[test]
    fn test_index_transactions() -> ResultTest<()> {
        let (mut stdb, _tmp_dir) = make_test_db()?;
        let table_id = _create_data(&mut stdb, 0..9)?;

        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        stdb.insert(tx, table_id, product![TypeValue::I32(9), TypeValue::String("A".into())])?;

        let idx = IndexDef::new("idx_1", table_id, 1, false);
        let mut idx = BTreeIndex::from_def(0.into(), idx);
        idx.index_full_column(&stdb, tx)?;

        let key = TypeValue::String("A".into());
        let mut values = _find(&idx, &key, &stdb, tx)?;
        values.sort();
        assert_eq!(
            values,
            vec![
                vec![TypeValue::I32(0), TypeValue::String("A".into())],
                vec![TypeValue::I32(3), TypeValue::String("A".into())],
                vec![TypeValue::I32(6), TypeValue::String("A".into())],
                vec![TypeValue::I32(9), TypeValue::String("A".into())]
            ]
        );

        stdb.rollback_tx(tx.clone());

        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let mut values = _find(&idx, &key, &stdb, tx)?;
        values.sort();
        assert_eq!(
            values,
            vec![
                vec![TypeValue::I32(0), TypeValue::String("A".into())],
                vec![TypeValue::I32(3), TypeValue::String("A".into())],
                vec![TypeValue::I32(6), TypeValue::String("A".into())]
            ]
        );

        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        stdb.insert(tx, table_id, product![TypeValue::I32(9), TypeValue::String("A".into())])?;

        let mut values = _find(&idx, &key, &stdb, tx)?;
        values.sort();
        assert_eq!(
            values,
            vec![
                vec![TypeValue::I32(0), TypeValue::String("A".into())],
                vec![TypeValue::I32(3), TypeValue::String("A".into())],
                vec![TypeValue::I32(6), TypeValue::String("A".into())],
                vec![TypeValue::I32(9), TypeValue::String("A".into())]
            ]
        );
        Ok(())
    }

    #[test]
    fn test_index_check_member() -> ResultTest<()> {
        let (mut stdb, _tmp_dir) = make_test_db()?;
        let table_id = _create_data(&mut stdb, 0..9)?;

        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let idx = IndexDef::new("idx_1", table_id, 0, false);
        let mut idx = BTreeIndex::from_def(0.into(), idx);
        idx.index_full_column(stdb, tx)?;

        assert!(!idx.contains_key(&TypeValue::I32(10)), "Key 10 found?");
        for i in 0..9 {
            assert!(idx.contains_key(&TypeValue::I32(i)), "Key  not found {}", i);
        }
        Ok(())
    }

    #[test]
    fn test_index_loaded() -> ResultTest<()> {
        let (mut stdb, _tmp_dir) = make_test_db()?;
        let table_id = _create_data(&mut stdb, 0..9)?;

        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let idx = IndexDef::new("idx_1", table_id, 0, true);
        stdb.create_index(tx, idx)?;

        let idx = stdb.catalog.indexes.get("idx_1").unwrap();
        assert_eq!(idx.len(), 9, "Wrong number of index unique keys");
        Ok(())
    }

    #[test]
    fn test_index_updated() -> ResultTest<()> {
        let (mut stdb, _tmp_dir) = make_test_db()?;

        let table_id = _create_data(&mut stdb, 0..9)?;

        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let idx_col_id = IndexDef::new("idx_1", table_id, 0, true);
        let idx_col_name = IndexDef::new("idx_2", table_id, 1, false);

        stdb.create_index(tx, idx_col_id)?;
        stdb.create_index(tx, idx_col_name)?;

        // Adding rows must update the indexes
        stdb.insert(
            tx,
            table_id,
            product![TypeValue::I32(99), TypeValue::String("NEW".into())],
        )?;

        let idx_col_id = stdb.catalog.indexes.get("idx_1").expect("Index not found");
        let idx_col_name = stdb.catalog.indexes.get("idx_2").expect("Index not found");

        let key = TypeValue::I32(99);
        let mut values = _find(&idx_col_id, &key, &stdb, tx)?;
        values.sort();
        assert_eq!(values, vec![vec![TypeValue::I32(99), TypeValue::String("NEW".into())],]);

        let key = TypeValue::String("NEW".into());
        let mut values = _find(&idx_col_name, &key, &stdb, tx)?;
        values.sort();
        assert_eq!(values, vec![vec![TypeValue::I32(99), TypeValue::String("NEW".into())],]);
        Ok(())
    }

    #[test]
    fn test_index_delete_rows() -> ResultTest<()> {
        let (mut stdb, _tmp_dir) = make_test_db()?;

        let table_id = _create_data(&mut stdb, 0..9)?;

        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let idx_col_id = IndexDef::new("idx_1", table_id, 0, true);

        stdb.create_index(tx, idx_col_id)?;

        let idx_col_id = stdb.catalog.indexes.get("idx_1").expect("Index not found");

        let mut all_rows = Vec::new();
        for row in stdb.scan(tx, table_id)? {
            all_rows.push(row)
        }
        all_rows.sort();

        let rows_col_id = idx_col_id.iter(&stdb, tx).collect::<Vec<_>>();
        assert_eq!(all_rows, rows_col_id);

        stdb.delete_in(tx, table_id, all_rows)?;

        let idx_col_id = stdb.catalog.indexes.get("idx_1").expect("Index not found");

        let rows_col_id = idx_col_id.iter(&stdb, tx).collect::<Vec<_>>();
        let empty: Vec<TupleValue> = Vec::new();

        assert_eq!(empty, rows_col_id);
        Ok(())
    }

    #[test]
    fn test_index_iter() -> ResultTest<()> {
        let (mut stdb, _tmp_dir) = make_test_db()?;

        let table_id = _create_data(&mut stdb, 0..9)?;

        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let idx_col_id = IndexDef::new("idx_1", table_id, 0, true);
        let idx_col_name = IndexDef::new("idx_2", table_id, 1, false);

        stdb.create_index(tx, idx_col_id)?;
        stdb.create_index(tx, idx_col_name)?;

        let mut all_rows = Vec::new();
        for row in stdb.scan(tx, table_id)? {
            all_rows.push(row)
        }
        all_rows.sort();

        let idx_col_id = stdb.catalog.indexes.get("idx_1").expect("Index not found");
        let idx_col_name = stdb.catalog.indexes.get("idx_2").expect("Index not found");

        let rows_col_id = idx_col_id.iter(&stdb, tx).collect::<Vec<_>>();
        assert_eq!(all_rows, rows_col_id);

        let mut rows_col_name = idx_col_name.iter(&stdb, tx).collect::<Vec<_>>();
        rows_col_name.sort();

        assert_eq!(all_rows, rows_col_name);

        let mut rows_col_id = stdb.scan_index(tx, table_id).unwrap().collect::<Vec<_>>();
        rows_col_id.sort();

        assert_eq!(all_rows, rows_col_id);
        Ok(())
    }

    #[test]
    fn test_index_iter_range() -> ResultTest<()> {
        let (mut stdb, _tmp_dir) = make_test_db()?;

        let table_id = _create_data(&mut stdb, 0..9)?;

        let mut tx_ = stdb.begin_tx();
        let (tx, stdb) = tx_.get();

        let idx_col_id = IndexDef::new("idx_1", table_id, 0, true);
        let idx_col_name = IndexDef::new("idx_2", table_id, 1, false);

        stdb.create_index(tx, idx_col_id)?;
        stdb.create_index(tx, idx_col_name)?;

        let mut all_rows = Vec::new();
        for row in stdb.scan(tx, table_id)? {
            all_rows.push(row.elements)
        }
        all_rows.sort();

        let idx_col_id = stdb.catalog.indexes.get("idx_1").expect("Index not found");

        let rows_col_id = idx_col_id
            .iter_range(&stdb, tx, TypeValue::I32(0)..=TypeValue::I32(1))
            .collect::<Vec<_>>();

        assert_eq!(
            vec![
                product![TypeValue::I32(0), TypeValue::String("A".into())],
                product![TypeValue::I32(1), TypeValue::String("B".into())],
            ],
            rows_col_id
        );

        let rows_col_id = stdb
            .range_scan_index(tx, table_id, 0, TypeValue::I32(0)..=TypeValue::I32(1))
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(
            vec![
                product![TypeValue::I32(0), TypeValue::String("A".into())],
                product![TypeValue::I32(1), TypeValue::String("B".into())],
            ],
            rows_col_id
        );
        Ok(())
    }
}
