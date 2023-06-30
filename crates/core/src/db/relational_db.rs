use super::commit_log::CommitLog;
use super::datastore::locking_tx_datastore::{Data, DataRef, Iter, IterByColEq, IterByColRange, MutTxId, RowId};
use super::datastore::traits::{
    ColId, DataRow, IndexDef, IndexId, MutTx, MutTxDatastore, SequenceDef, SequenceId, TableDef, TableId, TableSchema,
    TxData,
};
use super::message_log::MessageLog;
use super::relational_operators::Relation;
use crate::db::db_metrics::{RDB_DELETE_BY_REL_TIME, RDB_DROP_TABLE_TIME, RDB_INSERT_TIME, RDB_ITER_TIME};
use crate::db::messages::commit::Commit;
use crate::db::ostorage::hashmap_object_db::HashMapObjectDB;
use crate::db::ostorage::ObjectDB;
use crate::error::{DBError, DatabaseError, TableError};
use crate::hash::Hash;
use crate::util::prometheus_handle::HistogramVecHandle;
use fs2::FileExt;
use prometheus::HistogramVec;
use spacetimedb_lib::ColumnIndexAttribute;
use spacetimedb_lib::{data_key::ToDataKey, PrimaryKey};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductType, ProductValue};
use std::fs::File;
use std::ops::RangeBounds;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::datastore::locking_tx_datastore::Locking;

/// Starts histogram prometheus measurements for `table_id`.
fn measure(hist: &'static HistogramVec, table_id: u32) {
    HistogramVecHandle::new(hist, vec![format!("{}", table_id)]).start();
}

pub const ST_TABLES_NAME: &str = "st_table";
pub const ST_COLUMNS_NAME: &str = "st_columns";
pub const ST_SEQUENCES_NAME: &str = "st_sequence";
pub const ST_INDEXES_NAME: &str = "st_indexes";

/// The static ID of the table that defines tables
pub const ST_TABLES_ID: u32 = 0;
/// The static ID of the table that defines columns
pub const ST_COLUMNS_ID: u32 = 1;
/// The ID that we can start use to generate user tables that will not conflict with the bootstrapped ones.
pub const ST_TABLE_ID_START: u32 = 2;

#[derive(Clone)]
pub struct RelationalDB {
    // TODO(cloutiertyler): This should not be public
    pub(crate) inner: Locking,
    commit_log: CommitLog,
    _lock: Arc<File>,
}

impl DataRow for RelationalDB {
    type RowId = RowId;
    type Data = Data;
    type DataRef = DataRef;

    fn data_to_owned(&self, data_ref: Self::DataRef) -> Self::Data {
        self.inner.data_to_owned(data_ref)
    }
}

impl std::fmt::Debug for RelationalDB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelationalDB").finish()
    }
}

impl RelationalDB {
    pub fn open(
        root: impl AsRef<Path>,
        message_log: Arc<Mutex<MessageLog>>,
        odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
    ) -> Result<Self, DBError> {
        log::trace!("DATABASE: OPENING");

        // NOTE: This prevents accidentally opening the same database twice
        // which could potentially cause corruption if commits were interleaved
        // and so forth
        let root = root.as_ref();
        let lock = File::create(root.join("db.lock"))?;
        lock.try_lock_exclusive()
            .map_err(|err| DatabaseError::DatabasedOpened(root.to_path_buf(), err.into()))?;

        let datastore = Locking::bootstrap()?;
        let unwritten_commit = {
            let message_log = message_log.lock().unwrap();
            let mut transaction_offset = 0;
            let mut last_commit_offset = None;
            let mut last_hash: Option<Hash> = None;
            for message in message_log.iter() {
                let (commit, _) = Commit::decode(message);
                last_hash = commit.parent_commit_hash;
                last_commit_offset = Some(commit.commit_offset);
                for transaction in commit.transactions {
                    transaction_offset += 1;
                    // NOTE: Although I am creating a blobstore transaction in a
                    // one to one fashion for each message log transaction, this
                    // is just to reduce memory usage while inserting. We don't
                    // really care about inserting these transactionally as long
                    // as all of the writes get inserted.
                    datastore.replay_transaction(&transaction, odb.clone())?;
                }
            }

            // The purpose of this is to rebuild the state of the datastore
            // after having inserted all of rows from the message log.
            // This is necessary because, for example, inserting a row into `st_table`
            // is not equivalent to calling `create_table`.
            // There may eventually be better way to do this, but this will have to do for now.
            datastore.rebuild_state_after_replay()?;

            let commit_offset = if let Some(last_commit_offset) = last_commit_offset {
                last_commit_offset + 1
            } else {
                0
            };

            log::debug!(
                "Initialized with {} commits and tx offset {}",
                commit_offset,
                transaction_offset
            );

            Commit {
                parent_commit_hash: last_hash,
                commit_offset,
                min_tx_offset: transaction_offset,
                transactions: Vec::new(),
            }
        };
        let commit_log = CommitLog::new(message_log, odb.clone(), unwritten_commit);

        // i.e. essentially bootstrap the creation of the schema
        // tables by hard coding the schema of the schema tables
        let db = Self {
            inner: datastore,
            commit_log,
            _lock: Arc::new(lock),
        };

        log::trace!("DATABASE: OPENED");
        Ok(db)
    }

    // pub fn reset_hard(&mut self, message_log: Arc<Mutex<MessageLog>>) -> Result<(), DBError> {
    //     log::warn!("DATABASE: RESET");

    //     self.txdb.reset_hard(message_log)?;
    //     Ok(())
    // }

    #[tracing::instrument(skip_all)]
    pub fn pk_for_row(row: &ProductValue) -> PrimaryKey {
        PrimaryKey {
            data_key: row.to_data_key(),
        }
    }

    #[tracing::instrument(skip_all)]
    pub fn encode_row(row: &ProductValue, bytes: &mut Vec<u8>) {
        // TODO: large file storage of the row elements
        row.encode(bytes);
    }

    #[tracing::instrument(skip_all)]
    pub fn schema_for_table(&self, tx: &MutTxId, table_id: u32) -> Result<TableSchema, DBError> {
        self.inner.schema_for_table_mut_tx(tx, TableId(table_id))
    }

    #[tracing::instrument(skip_all)]
    pub fn row_schema_for_table(&self, tx: &MutTxId, table_id: u32) -> Result<ProductType, DBError> {
        self.inner.row_type_for_table_mut_tx(tx, TableId(table_id))
    }

    pub fn get_all_tables(&self, tx: &MutTxId) -> Result<Vec<TableSchema>, DBError> {
        self.inner.get_all_tables_mut_tx(tx)
    }

    #[tracing::instrument(skip_all)]
    pub fn schema_for_column(&self, tx: &MutTxId, table_id: u32, col_id: u32) -> Result<AlgebraicType, DBError> {
        let schema = self.row_schema_for_table(tx, table_id)?;
        let col = schema
            .elements
            .get(col_id as usize)
            .ok_or(TableError::ColumnNotFound(col_id))?;
        Ok(col.algebraic_type.clone())
    }

    pub fn decode_column(
        &self,
        tx: &MutTxId,
        table_id: u32,
        col_id: u32,
        bytes: &[u8],
    ) -> Result<AlgebraicValue, DBError> {
        let schema = self.schema_for_column(tx, table_id, col_id)?;
        Ok(AlgebraicValue::decode(&schema, &mut &bytes[..])?)
    }

    /// Begin a transaction.
    ///
    /// **Note**: this call **must** be paired with [`Self::rollback_tx`] or
    /// [`Self::commit_tx`], otherwise the database will be left in an invalid
    /// state. See also [`Self::with_auto_commit`].
    pub fn begin_tx(&self) -> MutTxId {
        log::trace!("BEGIN TX");
        self.inner.begin_mut_tx()
    }

    pub fn rollback_tx(&self, tx: MutTxId) {
        log::trace!("ROLLBACK TX");
        self.inner.rollback_mut_tx(tx)
    }
    pub fn commit_tx(&self, tx: MutTxId) -> Result<Option<(TxData, Option<usize>)>, DBError> {
        log::trace!("COMMIT TX");
        if let Some(tx_data) = self.inner.commit_mut_tx(tx)? {
            let bytes_written = self.commit_log.append_tx(&tx_data, &self.inner)?;
            return Ok(Some((tx_data, bytes_written)));
        }
        Ok(None)
    }

    /// Run a fallible function in a transaction.
    ///
    /// If the supplied function returns `Ok`, the transaction is automatically
    /// committed. Otherwise, the transaction is rolled back.
    ///
    /// This method is provided for convenience, as it allows to safely use the
    /// `?` operator in code running within a transaction context. Recall that a
    /// [`MutTxId`] does not follow the RAII pattern, so the following code is
    /// wrong:
    ///
    /// ```ignore
    /// let tx = db.begin_tx();
    /// let _ = db.schema_for_table(tx, 42)?;
    /// // ...
    /// let _ = db.commit_tx(tx)?;
    /// ```
    ///
    /// If `schema_for_table` returns an error, the transaction is not properly
    /// cleaned up, as the `?` short-circuits. To avoid this, but still be able
    /// to use `?`, you can write:
    ///
    /// ```ignore
    /// db.with_auto_commit(|tx| {
    ///     let _ = db.schema_for_table(tx, 42)?;
    ///     // ...
    ///     Ok(())
    /// })?;
    /// ```
    pub fn with_auto_commit<F, A, E>(&self, f: F) -> Result<A, E>
    where
        F: FnOnce(&mut MutTxId) -> Result<A, E>,
        E: From<DBError>,
    {
        let mut tx = self.begin_tx();
        let res = f(&mut tx);
        if res.is_err() {
            self.rollback_tx(tx);
        } else {
            match self.commit_tx(tx).map_err(E::from)? {
                Some(_) => (),
                None => panic!("TODO: retry?"),
            }
        }
        res
    }
}

impl RelationalDB {
    pub fn create_table<T: Into<TableDef>>(&self, tx: &mut MutTxId, schema: T) -> Result<u32, DBError> {
        self.inner.create_table_mut_tx(tx, schema.into()).map(|TableId(id)| id)
    }

    pub fn drop_table(&self, tx: &mut MutTxId, table_id: u32) -> Result<(), DBError> {
        measure(&RDB_DROP_TABLE_TIME, table_id);
        self.inner.drop_table_mut_tx(tx, TableId(table_id))
    }

    /// Rename a table.
    ///
    /// Sets the name of the table to `new_name` regardless of the previous value. This is a
    /// relatively cheap operation which only modifies the system tables.
    ///
    /// If the table is not found or is a system table, an error is returned.
    pub fn rename_table(&self, tx: &mut MutTxId, table_id: u32, new_name: &str) -> Result<(), DBError> {
        self.inner.rename_table_mut_tx(tx, TableId(table_id), new_name)
    }

    #[tracing::instrument(skip_all)]
    pub fn table_id_from_name(&self, tx: &MutTxId, table_name: &str) -> Result<Option<u32>, DBError> {
        self.inner
            .table_id_from_name_mut_tx(tx, table_name)
            .map(|x| x.map(|x| x.0))
    }

    #[tracing::instrument(skip_all)]
    pub fn table_exist(&self, tx: &MutTxId, table_name: &str) -> Result<Option<u32>, DBError> {
        self.inner
            .table_id_from_name_mut_tx(tx, table_name)
            .map(|x| x.map(|x| x.0))
    }

    #[tracing::instrument(skip_all)]
    pub fn table_name_from_id(&self, tx: &MutTxId, table_id: u32) -> Result<Option<String>, DBError> {
        self.inner.table_name_from_id_mut_tx(tx, TableId(table_id))
    }

    #[tracing::instrument(skip_all)]
    pub fn column_attrs(
        &self,
        tx: &mut MutTxId,
        table_id: u32,
        col_id: u32,
    ) -> Result<Option<ColumnIndexAttribute>, DBError> {
        let table = self.inner.schema_for_table_mut_tx(tx, TableId(table_id))?;
        let Some(column) = table.columns.get(col_id as usize) else { return Ok(None) };
        let unique_index = table.indexes.iter().find(|x| x.col_id == col_id).map(|x| x.is_unique);
        Ok(Some(match (column.is_autoinc, unique_index) {
            (true, Some(true)) => ColumnIndexAttribute::Identity,
            (true, Some(false) | None) => ColumnIndexAttribute::AutoInc,
            (false, Some(true)) => ColumnIndexAttribute::Unique,
            (false, Some(false)) => ColumnIndexAttribute::Indexed,
            (false, None) => ColumnIndexAttribute::UnSet,
        }))
    }

    #[tracing::instrument(skip_all)]
    pub fn index_id_from_name(&self, tx: &MutTxId, index_name: &str) -> Result<Option<u32>, DBError> {
        self.inner
            .index_id_from_name_mut_tx(tx, index_name)
            .map(|x| x.map(|x| x.0))
    }

    #[tracing::instrument(skip_all)]
    pub fn sequence_id_from_name(&self, tx: &MutTxId, sequence_name: &str) -> Result<Option<u32>, DBError> {
        self.inner
            .sequence_id_from_name_mut_tx(tx, sequence_name)
            .map(|x| x.map(|x| x.0))
    }

    /// Adds the [index::BTreeIndex] into the [ST_INDEXES_NAME] table
    ///
    /// Returns the `index_id`
    ///
    /// NOTE: It loads the data from the table into it before returning
    #[tracing::instrument(skip(self, tx))]
    pub fn create_index(&self, tx: &mut MutTxId, index: IndexDef) -> Result<IndexId, DBError> {
        self.inner.create_index_mut_tx(tx, index)
    }

    /// Removes the [index::BTreeIndex] from the database by their `index_id`
    #[tracing::instrument(skip(self, tx))]
    pub fn drop_index(&self, tx: &mut MutTxId, index_id: IndexId) -> Result<(), DBError> {
        self.inner.drop_index_mut_tx(tx, index_id)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`.
    #[tracing::instrument(skip(self, tx))]
    pub fn iter<'a>(&'a self, tx: &'a MutTxId, table_id: u32) -> Result<Iter<'a>, DBError> {
        measure(&RDB_ITER_TIME, table_id);
        self.inner.iter_mut_tx(tx, TableId(table_id))
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `col_id` matches `value`.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    #[tracing::instrument(skip(self, tx))]
    pub fn iter_by_col_eq<'a>(
        &'a self,
        tx: &'a mut MutTxId,
        table_id: u32,
        col_id: u32,
        value: &'a AlgebraicValue,
    ) -> Result<IterByColEq<'a>, DBError> {
        self.inner
            .iter_by_col_eq_mut_tx(tx, TableId(table_id), ColId(col_id), value)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `col_id` matches what is within `range`.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue> + 'a>(
        &'a self,
        tx: &'a MutTxId,
        table_id: u32,
        col_id: u32,
        range: R,
    ) -> Result<IterByColRange<'a, R>, DBError> {
        self.inner
            .iter_by_col_range_mut_tx(tx, TableId(table_id), ColId(col_id), range)
    }

    #[tracing::instrument(skip(self, tx))]
    pub fn insert(&self, tx: &mut MutTxId, table_id: u32, row: ProductValue) -> Result<ProductValue, DBError> {
        measure(&RDB_INSERT_TIME, table_id);
        self.inner.insert_mut_tx(tx, TableId(table_id), row)
    }

    #[tracing::instrument(skip_all)]
    pub fn insert_bytes_as_row(
        &self,
        tx: &mut MutTxId,
        table_id: u32,
        row_bytes: &[u8],
    ) -> Result<ProductValue, DBError> {
        let ty = self.inner.row_type_for_table_mut_tx(tx, TableId(table_id))?;
        let row = ProductValue::decode(&ty, &mut &row_bytes[..])?;
        self.insert(tx, table_id, row)
    }

    /*
    #[tracing::instrument(skip_all)]
    pub fn delete_pk(&self, tx: &mut MutTxId, table_id: u32, row_id: DataKey) -> Result<bool, DBError> {
        measure(&RDB_DELETE_PK_TIME, table_id);
        self.inner.delete_row_mut_tx(tx, TableId(table_id), RowId(row_id))
    }
    */

    #[tracing::instrument(skip_all)]
    pub fn delete_by_rel<R: Relation>(
        &self,
        tx: &mut MutTxId,
        table_id: u32,
        relation: R,
    ) -> Result<Option<u32>, DBError> {
        measure(&RDB_DELETE_BY_REL_TIME, table_id);
        self.inner.delete_by_rel_mut_tx(tx, TableId(table_id), relation)
    }

    /// Generated the next value for the [SequenceId]
    #[tracing::instrument(skip_all)]
    pub fn next_sequence(&mut self, tx: &mut MutTxId, seq_id: SequenceId) -> Result<i128, DBError> {
        self.inner.get_next_sequence_value_mut_tx(tx, seq_id)
    }

    /// Add a [Sequence] into the database instance, generates a stable [SequenceId] for it that will persist on restart.
    #[tracing::instrument(skip(self, tx))]
    pub fn create_sequence(&mut self, tx: &mut MutTxId, seq: SequenceDef) -> Result<SequenceId, DBError> {
        self.inner.create_sequence_mut_tx(tx, seq)
    }

    ///Removes the [Sequence] from database instance
    #[tracing::instrument(skip(self, tx))]
    pub fn drop_sequence(&self, tx: &mut MutTxId, seq_id: SequenceId) -> Result<(), DBError> {
        self.inner.drop_sequence_mut_tx(tx, seq_id)
    }
}

pub fn make_default_ostorage(path: impl AsRef<Path>) -> Result<Box<dyn ObjectDB + Send>, DBError> {
    Ok(Box::new(HashMapObjectDB::open(path)?))
}

pub fn open_db(path: impl AsRef<Path>) -> Result<RelationalDB, DBError> {
    let path = path.as_ref();
    let mlog = Arc::new(Mutex::new(MessageLog::open(path.join("mlog"))?));
    let odb = Arc::new(Mutex::new(make_default_ostorage(path.join("odb"))?));
    let stdb = RelationalDB::open(path, mlog, odb)?;

    Ok(stdb)
}

pub fn open_log(path: impl AsRef<Path>) -> Result<Arc<Mutex<MessageLog>>, DBError> {
    let path = path.as_ref().to_path_buf();
    Ok(Arc::new(Mutex::new(MessageLog::open(path.join("mlog"))?)))
}

#[cfg(test)]
pub(crate) mod tests_utils {
    use super::*;
    use tempdir::TempDir;

    //Utility for creating a database on a TempDir
    pub(crate) fn make_test_db() -> Result<(RelationalDB, TempDir), DBError> {
        let tmp_dir = TempDir::new("stdb_test")?;
        let stdb = open_db(&tmp_dir)?;
        Ok((stdb, tmp_dir))
    }
}

#[cfg(test)]
mod tests {

    use std::sync::{Arc, Mutex};

    use crate::db::datastore::system_tables::StIndexRow;
    use crate::db::datastore::system_tables::StSequenceRow;
    use crate::db::datastore::system_tables::StTableRow;
    use crate::db::datastore::system_tables::ST_INDEXES_ID;
    use crate::db::datastore::system_tables::ST_SEQUENCES_ID;
    use crate::db::datastore::traits::ColumnDef;
    use crate::db::datastore::traits::IndexDef;
    use crate::db::datastore::traits::TableDef;
    use crate::db::message_log::MessageLog;
    use crate::db::relational_db::ST_TABLES_ID;

    use super::RelationalDB;
    use crate::db::relational_db::make_default_ostorage;
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::error::{DBError, DatabaseError, IndexError};
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::{AlgebraicType, AlgebraicValue, ProductType};
    use spacetimedb_sats::product;

    #[test]
    fn test() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();
        let mut schema = TableDef::from(ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        schema.table_name = "MyTable".to_string();
        stdb.create_table(&mut tx, schema)?;
        stdb.commit_tx(tx)?;

        Ok(())
    }

    #[test]
    fn test_open_twice() -> ResultTest<()> {
        let (stdb, tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();

        let mut schema = TableDef::from(ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        schema.table_name = "MyTable".to_string();
        stdb.create_table(&mut tx, schema)?;

        stdb.commit_tx(tx)?;

        let mlog = Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?));
        let odb = Arc::new(Mutex::new(make_default_ostorage(tmp_dir.path().join("odb"))?));

        match RelationalDB::open(tmp_dir.path(), mlog, odb) {
            Ok(_) => {
                panic!("Allowed to open database twice")
            }
            Err(e) => match e {
                DBError::Database(DatabaseError::DatabasedOpened(_, _)) => {}
                err => {
                    panic!("Failed with error {err}")
                }
            },
        }

        Ok(())
    }

    #[test]
    fn test_table_name() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();
        let mut schema = TableDef::from(ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        schema.table_name = "MyTable".to_string();
        let table_id = stdb.create_table(&mut tx, schema)?;
        let t_id = stdb.table_id_from_name(&tx, "MyTable")?;
        assert_eq!(t_id, Some(table_id));
        Ok(())
    }

    #[test]
    fn test_column_name() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();
        let mut schema = TableDef::from(ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        schema.table_name = "MyTable".to_string();
        stdb.create_table(&mut tx, schema)?;
        let table_id = stdb.table_id_from_name(&tx, "MyTable")?.unwrap();
        let schema = stdb.schema_for_table(&tx, table_id)?;
        let col = schema.columns.iter().find(|x| x.col_name == "my_col").unwrap();
        assert_eq!(col.col_id, 0);
        Ok(())
    }

    #[test]
    fn test_create_table_pre_commit() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();
        let mut schema = TableDef::from(ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        schema.table_name = "MyTable".to_string();
        stdb.create_table(&mut tx, schema.clone())?;
        let result = stdb.create_table(&mut tx, schema);
        assert!(matches!(result, Err(_)));
        Ok(())
    }

    #[test]
    fn test_pre_commit() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();

        let mut schema = TableDef::from(ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        schema.table_name = "MyTable".to_string();
        let table_id = stdb.create_table(&mut tx, schema)?;

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(-1)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(1)])?;

        let mut rows = stdb
            .iter(&tx, table_id)?
            .map(|r| *r.view().elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![-1, 0, 1]);
        Ok(())
    }

    #[test]
    fn test_post_commit() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();

        let mut schema = TableDef::from(ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        schema.table_name = "MyTable".to_string();
        let table_id = stdb.create_table(&mut tx, schema)?;

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(-1)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(1)])?;
        stdb.commit_tx(tx)?;

        let tx = stdb.begin_tx();
        let mut rows = stdb
            .iter(&tx, table_id)?
            .map(|r| *r.view().elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![-1, 0, 1]);
        Ok(())
    }

    #[test]
    fn test_filter_range_pre_commit() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();

        let mut schema = TableDef::from(ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        schema.table_name = "MyTable".to_string();
        let table_id = stdb.create_table(&mut tx, schema)?;

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(-1)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(1)])?;

        let mut rows = stdb
            .iter_by_col_range(&tx, table_id, 0, AlgebraicValue::I32(0)..)?
            .map(|r| *r.view().elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn test_filter_range_post_commit() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();

        let mut schema = TableDef::from(ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        schema.table_name = "MyTable".to_string();
        let table_id = stdb.create_table(&mut tx, schema)?;

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(-1)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(1)])?;
        stdb.commit_tx(tx)?;

        let tx = stdb.begin_tx();
        let mut rows = stdb
            .iter_by_col_range(&tx, table_id, 0, AlgebraicValue::I32(0)..)?
            .map(|r| *r.view().elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn test_create_table_rollback() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();

        let mut schema = TableDef::from(ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        schema.table_name = "MyTable".to_string();
        let table_id = stdb.create_table(&mut tx, schema)?;
        stdb.rollback_tx(tx);

        let mut tx = stdb.begin_tx();
        let result = stdb.drop_table(&mut tx, table_id);
        assert!(matches!(result, Err(_)));
        Ok(())
    }

    #[test]
    fn test_rollback() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();

        let mut schema = TableDef::from(ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        schema.table_name = "MyTable".to_string();
        let table_id = stdb.create_table(&mut tx, schema)?;
        stdb.commit_tx(tx)?;

        let mut tx = stdb.begin_tx();
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(-1)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(1)])?;
        stdb.rollback_tx(tx);

        let tx = stdb.begin_tx();
        let mut rows = stdb
            .iter(&tx, table_id)?
            .map(|r| *r.view().elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        let expected: Vec<i32> = Vec::new();
        assert_eq!(rows, expected);
        Ok(())
    }

    #[test]
    fn test_auto_inc() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();
        let schema = TableDef {
            table_name: "MyTable".to_string(),
            columns: vec![ColumnDef {
                col_name: "my_col".to_string(),
                col_type: AlgebraicType::I64,
                is_autoinc: true,
            }],
            indexes: vec![],
        };
        let table_id = stdb.create_table(&mut tx, schema)?;

        let sequence = stdb.sequence_id_from_name(&tx, "MyTable_my_col_seq")?;
        assert!(sequence.is_some(), "Sequence not created");

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(0)])?;

        let mut rows = stdb
            .iter_by_col_range(&tx, table_id, 0, AlgebraicValue::I64(0)..)?
            .map(|r| *r.view().elements[0].as_i64().unwrap())
            .collect::<Vec<i64>>();
        rows.sort();

        assert_eq!(rows, vec![1, 2]);

        Ok(())
    }

    #[test]
    fn test_auto_inc_disable() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();
        let schema = TableDef {
            table_name: "MyTable".to_string(),
            columns: vec![ColumnDef {
                col_name: "my_col".to_string(),
                col_type: AlgebraicType::I64,
                is_autoinc: true,
            }],
            indexes: vec![],
        };
        let table_id = stdb.create_table(&mut tx, schema)?;

        let sequence = stdb.sequence_id_from_name(&tx, "MyTable_my_col_seq")?;
        assert!(sequence.is_some(), "Sequence not created");

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(5)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(6)])?;

        let mut rows = stdb
            .iter_by_col_range(&tx, table_id, 0, AlgebraicValue::I64(0)..)?
            .map(|r| *r.view().elements[0].as_i64().unwrap())
            .collect::<Vec<i64>>();
        rows.sort();

        assert_eq!(rows, vec![5, 6]);

        Ok(())
    }

    #[test]
    fn test_indexed() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();
        let schema = TableDef {
            table_name: "MyTable".to_string(),
            columns: vec![ColumnDef {
                col_name: "my_col".to_string(),
                col_type: AlgebraicType::I64,
                is_autoinc: false,
            }],
            indexes: vec![IndexDef {
                table_id: 0,
                col_id: 0,
                name: "MyTable_my_col_idx".to_string(),
                is_unique: false,
            }],
        };
        let table_id = stdb.create_table(&mut tx, schema)?;

        assert!(
            stdb.index_id_from_name(&tx, "MyTable_my_col_idx")?.is_some(),
            "Index not created"
        );

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(1)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(1)])?;

        let mut rows = stdb
            .iter_by_col_range(&tx, table_id, 0, AlgebraicValue::I64(0)..)?
            .map(|r| *r.view().elements[0].as_i64().unwrap())
            .collect::<Vec<i64>>();
        rows.sort();

        assert_eq!(rows, vec![1]);

        Ok(())
    }

    #[test]
    fn test_unique() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();
        let schema = TableDef {
            table_name: "MyTable".to_string(),
            columns: vec![ColumnDef {
                col_name: "my_col".to_string(),
                col_type: AlgebraicType::I64,
                is_autoinc: false,
            }],
            indexes: vec![IndexDef {
                table_id: 0,
                col_id: 0,
                name: "MyTable_my_col_idx".to_string(),
                is_unique: true,
            }],
        };
        let table_id = stdb.create_table(&mut tx, schema)?;

        assert!(
            stdb.index_id_from_name(&tx, "MyTable_my_col_idx")?.is_some(),
            "Index not created"
        );

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(1)])?;
        match stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(1)]) {
            Ok(_) => {
                panic!("Allow to insert duplicate row")
            }
            Err(DBError::Index(err)) => match err {
                IndexError::UniqueConstraintViolation { .. } => {}
                err => {
                    panic!("Expected error `UniqueConstraintViolation`, got {err}")
                }
            },
            err => {
                panic!("Expected error `UniqueConstraintViolation`, got {err:?}")
            }
        }

        Ok(())
    }

    #[test]
    fn test_identity() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();
        let schema = TableDef {
            table_name: "MyTable".to_string(),
            columns: vec![ColumnDef {
                col_name: "my_col".to_string(),
                col_type: AlgebraicType::I64,
                is_autoinc: true,
            }],
            indexes: vec![IndexDef {
                table_id: 0,
                col_id: 0,
                name: "MyTable_my_col_idx".to_string(),
                is_unique: true,
            }],
        };
        let table_id = stdb.create_table(&mut tx, schema)?;

        assert!(
            stdb.index_id_from_name(&tx, "MyTable_my_col_idx")?.is_some(),
            "Index not created"
        );

        let sequence = stdb.sequence_id_from_name(&tx, "MyTable_my_col_seq")?;
        assert!(sequence.is_some(), "Sequence not created");

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(0)])?;

        let mut rows = stdb
            .iter_by_col_range(&tx, table_id, 0, AlgebraicValue::I64(0)..)?
            .map(|r| *r.view().elements[0].as_i64().unwrap())
            .collect::<Vec<i64>>();
        rows.sort();

        assert_eq!(rows, vec![1, 2]);

        Ok(())
    }

    #[test]
    fn test_cascade_drop_table() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();
        let schema = TableDef {
            table_name: "MyTable".to_string(),
            columns: vec![
                ColumnDef {
                    col_name: "col1".to_string(),
                    col_type: AlgebraicType::I64,
                    is_autoinc: false,
                },
                ColumnDef {
                    col_name: "col2".to_string(),
                    col_type: AlgebraicType::I64,
                    is_autoinc: true,
                },
                ColumnDef {
                    col_name: "col3".to_string(),
                    col_type: AlgebraicType::I64,
                    is_autoinc: false,
                },
                ColumnDef {
                    col_name: "col4".to_string(),
                    col_type: AlgebraicType::I64,
                    is_autoinc: true,
                },
            ],
            indexes: vec![
                IndexDef {
                    table_id: 0,
                    col_id: 0,
                    name: "MyTable_col1_idx".to_string(),
                    is_unique: true,
                },
                IndexDef {
                    table_id: 0,
                    col_id: 2,
                    name: "MyTable_col3_idx".to_string(),
                    is_unique: false,
                },
                IndexDef {
                    table_id: 0,
                    col_id: 3,
                    name: "MyTable_col4_idx".to_string(),
                    is_unique: true,
                },
            ],
        };
        let table_id = stdb.create_table(&mut tx, schema)?;

        let indexes = stdb
            .iter(&tx, ST_INDEXES_ID.0)?
            .map(|x| StIndexRow::try_from(x.view()).unwrap().to_owned())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(indexes.len(), 3, "Wrong number of indexes");

        let sequences = stdb
            .iter(&tx, ST_SEQUENCES_ID.0)?
            .map(|x| StSequenceRow::try_from(x.view()).unwrap().to_owned())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(sequences.len(), 2, "Wrong number of sequences");

        stdb.drop_table(&mut tx, table_id)?;

        let indexes = stdb
            .iter(&tx, ST_INDEXES_ID.0)?
            .map(|x| StIndexRow::try_from(x.view()).unwrap().to_owned())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(indexes.len(), 0, "Wrong number of indexes");

        let sequences = stdb
            .iter(&tx, ST_SEQUENCES_ID.0)?
            .map(|x| StSequenceRow::try_from(x.view()).unwrap().to_owned())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(sequences.len(), 0, "Wrong number of sequences");

        Ok(())
    }

    #[test]
    fn test_rename_table() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_tx();

        let schema = TableDef {
            table_name: "MyTable".to_string(),
            columns: vec![ColumnDef {
                col_name: "my_col".to_string(),
                col_type: AlgebraicType::I64,
                is_autoinc: true,
            }],
            indexes: vec![IndexDef {
                table_id: 0,
                col_id: 0,
                name: "MyTable_my_col_idx".to_string(),
                is_unique: true,
            }],
        };
        let table_id = stdb.create_table(&mut tx, schema)?;
        stdb.rename_table(&mut tx, table_id, "YourTable")?;
        let table_name = stdb.table_name_from_id(&tx, table_id)?;

        assert_eq!(Some("YourTable"), table_name.as_deref());
        // Also make sure we've removed the old ST_TABLES_ID row
        let mut n = 0;
        for row in stdb.iter(&tx, ST_TABLES_ID)? {
            let table = StTableRow::try_from(row.view())?;
            if table.table_id == table_id {
                n += 1;
            }
        }
        assert_eq!(1, n);

        Ok(())
    }

    // #[test]
    // fn test_rename_column() -> ResultTest<()> {
    //     let (mut stdb, _tmp_dir) = make_test_db()?;

    //     let mut tx_ = stdb.begin_tx();
    //     let (tx, stdb) = tx_.get();

    //     let schema = &[("col1", AlgebraicType::U64, ColumnIndexAttribute::Identity)];
    //     let table_id = stdb.create_table(tx, "MyTable", ProductTypeMeta::from_iter(&schema[..1]))?;
    //     let column_id = stdb.column_id_from_name(tx, table_id, "col1")?.unwrap();
    //     stdb.rename_column(tx, table_id, column_id, "id")?;

    //     assert_eq!(Some(column_id), stdb.column_id_from_name(tx, table_id, "id")?);
    //     assert_eq!(None, stdb.column_id_from_name(tx, table_id, "col1")?);

    //     Ok(())
    // }
}
