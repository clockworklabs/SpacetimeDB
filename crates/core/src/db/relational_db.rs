use super::commit_log::{CommitLog, CommitLogMut};
use super::datastore::locking_tx_datastore::{
    datastore::Locking,
    state_view::{Iter, IterByColEq, IterByColRange},
};
use super::datastore::traits::{
    MutProgrammable, MutTx as _, MutTxDatastore, Programmable, Tx as _, TxData, TxDatastore,
};
use super::message_log::MessageLog;
use super::ostorage::memory_object_db::MemoryObjectDB;
use super::relational_operators::Relation;
use crate::address::Address;
use crate::db::datastore::traits::DataRow;
use crate::db::db_metrics::DB_METRICS;
use crate::db::ostorage::hashmap_object_db::HashMapObjectDB;
use crate::db::ostorage::ObjectDB;
use crate::db::FsyncPolicy;
use crate::error::{DBError, DatabaseError, TableError};
use crate::execution_context::ExecutionContext;
use crate::hash::Hash;
use fs2::FileExt;
use spacetimedb_lib::PrimaryKey;
use spacetimedb_primitives::*;
use spacetimedb_sats::data_key::ToDataKey;
use spacetimedb_sats::db::def::{IndexDef, SequenceDef, TableDef, TableSchema};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductType, ProductValue};
use spacetimedb_table::{indexes::RowPointer, table::RowRef};
use std::borrow::Cow;
use std::fs::{create_dir_all, File};
use std::ops::RangeBounds;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub type MutTx = <Locking as super::datastore::traits::MutTx>::MutTx;
pub type Tx = <Locking as super::datastore::traits::Tx>::Tx;

type RowCountFn = Arc<dyn Fn(TableId, &str) -> i64 + Send + Sync>;

#[derive(Clone)]
pub struct RelationalDB {
    // TODO(cloutiertyler): This should not be public
    pub(crate) inner: Locking,
    commit_log: Option<CommitLogMut>,
    _lock: Arc<File>,
    address: Address,
    row_count_fn: RowCountFn,
}

impl DataRow for RelationalDB {
    type RowId = RowPointer;
    type DataRef<'a> = RowRef<'a>;

    fn view_product_value<'a>(&self, data_ref: Self::DataRef<'a>) -> Cow<'a, ProductValue> {
        Cow::Owned(data_ref.to_product_value())
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
        message_log: Option<Arc<Mutex<MessageLog>>>,
        odb: Arc<Mutex<Box<dyn ObjectDB + Send>>>,
        address: Address,
        fsync: bool,
    ) -> Result<Self, DBError> {
        let db_address = address;
        let address = address.to_hex();
        log::info!("[{}] DATABASE: OPENING", address);

        // Ensure that the `root` directory the database is running in exists.
        create_dir_all(&root)?;

        // NOTE: This prevents accidentally opening the same database twice
        // which could potentially cause corruption if commits were interleaved
        // and so forth
        let root = root.as_ref();
        let lock = File::create(root.join("db.lock"))?;
        lock.try_lock_exclusive()
            .map_err(|err| DatabaseError::DatabasedOpened(root.to_path_buf(), err.into()))?;

        let datastore = Locking::bootstrap(db_address)?;
        let mut transaction_offset = 0;
        let commit_log = message_log
            .map(|mlog| {
                log::info!("[{}] Replaying transaction log.", address);
                let mut last_logged_percentage = 0;

                let commit_log = CommitLog::new(mlog, odb);
                let max_commit_offset = commit_log.max_commit_offset();

                let commit_log = commit_log.replay(|commit, odb| {
                    transaction_offset += commit.transactions.len();
                    for transaction in commit.transactions {
                        datastore.replay_transaction(&transaction, odb)?;
                    }

                    let percentage =
                        f64::floor((commit.commit_offset as f64 / max_commit_offset as f64) * 100.0) as i32;
                    if percentage > last_logged_percentage && percentage % 10 == 0 {
                        last_logged_percentage = percentage;
                        log::info!(
                            "[{}] Loaded {}% ({}/{})",
                            address,
                            percentage,
                            transaction_offset,
                            max_commit_offset
                        );
                    }

                    Ok(())
                })?;

                let fsync = if fsync {
                    FsyncPolicy::EveryTx
                } else {
                    FsyncPolicy::Never
                };

                Ok::<_, DBError>(commit_log.with_fsync(fsync))
            })
            .transpose()?;

        // The purpose of this is to rebuild the state of the datastore
        // after having inserted all of rows from the message log.
        // This is necessary because, for example, inserting a row into `st_table`
        // is not equivalent to calling `create_table`.
        // There may eventually be better way to do this, but this will have to do for now.
        datastore.rebuild_state_after_replay()?;

        log::info!(
            "[{}] Initialized with {} commits and tx offset {}",
            address,
            commit_log.as_ref().map(|log| log.commit_offset()).unwrap_or_default(),
            transaction_offset
        );

        // i.e. essentially bootstrap the creation of the schema
        // tables by hard coding the schema of the schema tables
        let db = Self {
            inner: datastore,
            commit_log,
            _lock: Arc::new(lock),
            address: db_address,
            row_count_fn: Arc::new(move |table_id, table_name| {
                DB_METRICS
                    .rdb_num_table_rows
                    .with_label_values(&db_address, &table_id.into(), table_name)
                    .get()
            }),
        };

        log::info!("[{}] DATABASE: OPENED", address);
        Ok(db)
    }

    /// Returns an approximate row count for a particular table.
    /// TODO: Unify this with `Relation::row_count` when more statistics are added.
    pub fn row_count(&self, table_id: TableId, table_name: &str) -> i64 {
        (self.row_count_fn)(table_id, table_name)
    }

    /// Update this `RelationalDB` with an approximate row count function.
    pub fn with_row_count(mut self, row_count: RowCountFn) -> Self {
        self.row_count_fn = row_count;
        self
    }

    /// Returns the address for this database
    pub fn address(&self) -> Address {
        self.address
    }

    /// Obtain a read-only view of this database's [`CommitLog`].
    pub fn commit_log(&self) -> Option<CommitLog> {
        self.commit_log.as_ref().map(CommitLog::from)
    }

    /// The number of bytes on disk occupied by the [MessageLog].
    pub fn message_log_size_on_disk(&self) -> u64 {
        self.commit_log()
            .map_or(0, |commit_log| commit_log.message_log_size_on_disk())
    }

    /// The number of bytes on disk occupied by the [ObjectDB].
    pub fn object_db_size_on_disk(&self) -> std::result::Result<u64, DBError> {
        self.commit_log()
            .map_or(Ok(0), |commit_log| commit_log.object_db_size_on_disk())
    }

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
    pub fn schema_for_table_mut<'tx>(
        &self,
        tx: &'tx MutTx,
        table_id: TableId,
    ) -> Result<Cow<'tx, TableSchema>, DBError> {
        self.inner.schema_for_table_mut_tx(tx, table_id)
    }

    #[tracing::instrument(skip_all)]
    pub fn schema_for_table<'tx>(&self, tx: &'tx Tx, table_id: TableId) -> Result<Cow<'tx, TableSchema>, DBError> {
        self.inner.schema_for_table_tx(tx, table_id)
    }

    #[tracing::instrument(skip_all)]
    pub fn row_schema_for_table<'tx>(
        &self,
        tx: &'tx MutTx,
        table_id: TableId,
    ) -> Result<Cow<'tx, ProductType>, DBError> {
        self.inner.row_type_for_table_mut_tx(tx, table_id)
    }

    pub fn get_all_tables_mut<'tx>(&self, tx: &'tx MutTx) -> Result<Vec<Cow<'tx, TableSchema>>, DBError> {
        self.inner
            .get_all_tables_mut_tx(&ExecutionContext::internal(self.address), tx)
    }

    pub fn get_all_tables<'tx>(&self, tx: &'tx Tx) -> Result<Vec<Cow<'tx, TableSchema>>, DBError> {
        self.inner
            .get_all_tables_tx(&ExecutionContext::internal(self.address), tx)
    }

    #[tracing::instrument(skip_all)]
    pub fn schema_for_column<'tx>(
        &self,
        tx: &'tx MutTx,
        table_id: TableId,
        col_id: ColId,
    ) -> Result<Cow<'tx, AlgebraicType>, DBError> {
        // We need to do a manual bounds check here
        // since we want to do `swap_remove` to get an owned value
        // in the case of `Cow::Owned` and avoid a `clone`.
        let check_bounds = |schema: &ProductType| -> Result<_, DBError> {
            let col_idx = col_id.idx();
            if col_idx >= schema.elements.len() {
                return Err(TableError::ColumnNotFound(col_id).into());
            }
            Ok(col_idx)
        };
        Ok(match self.row_schema_for_table(tx, table_id)? {
            Cow::Borrowed(schema) => {
                let col_idx = check_bounds(schema)?;
                Cow::Borrowed(&schema.elements[col_idx].algebraic_type)
            }
            Cow::Owned(mut schema) => {
                let col_idx = check_bounds(&schema)?;
                Cow::Owned(schema.elements.swap_remove(col_idx).algebraic_type)
            }
        })
    }

    pub fn decode_column(
        &self,
        tx: &MutTx,
        table_id: TableId,
        col_id: ColId,
        bytes: &[u8],
    ) -> Result<AlgebraicValue, DBError> {
        let schema = self.schema_for_column(tx, table_id, col_id)?;
        Ok(AlgebraicValue::decode(&schema, &mut &*bytes)?)
    }

    /// Begin a transaction.
    ///
    /// **Note**: this call **must** be paired with [`Self::rollback_mut_tx`] or
    /// [`Self::commit_tx`], otherwise the database will be left in an invalid
    /// state. See also [`Self::with_auto_commit`].
    #[tracing::instrument(skip_all)]
    pub fn begin_mut_tx(&self) -> MutTx {
        log::trace!("BEGIN MUT TX");
        self.inner.begin_mut_tx()
    }

    #[tracing::instrument(skip_all)]
    pub fn begin_tx(&self) -> Tx {
        log::trace!("BEGIN TX");
        self.inner.begin_tx()
    }

    #[tracing::instrument(skip_all)]
    pub fn rollback_mut_tx(&self, ctx: &ExecutionContext, tx: MutTx) {
        log::trace!("ROLLBACK MUT TX");
        self.inner.rollback_mut_tx(ctx, tx)
    }

    #[tracing::instrument(skip_all)]
    pub fn release_tx(&self, ctx: &ExecutionContext, tx: Tx) {
        log::trace!("ROLLBACK TX");
        self.inner.release_tx(ctx, tx)
    }

    #[tracing::instrument(skip_all)]
    pub fn commit_tx(&self, ctx: &ExecutionContext, tx: MutTx) -> Result<Option<(TxData, Option<usize>)>, DBError> {
        log::trace!("COMMIT MUT TX");
        if let Some(tx_data) = self.inner.commit_mut_tx(ctx, tx)? {
            let bytes_written = self
                .commit_log
                .as_ref()
                .map(|commit_log| commit_log.append_tx(ctx, &tx_data, &self.inner))
                .transpose()?
                .flatten();
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
    /// [`MutTx`] does not follow the RAII pattern, so the following code is
    /// wrong:
    ///
    /// ```ignore
    /// let tx = db.begin_mut_tx();
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
    pub fn with_auto_commit<F, A, E>(&self, ctx: &ExecutionContext, f: F) -> Result<A, E>
    where
        F: FnOnce(&mut MutTx) -> Result<A, E>,
        E: From<DBError>,
    {
        let mut tx = self.begin_mut_tx();
        let res = f(&mut tx);
        self.finish_tx(ctx, tx, res)
    }

    /// Run a fallible function in a transaction, rolling it back if the
    /// function returns `Err`.
    ///
    /// Similar in purpose to [`Self::with_auto_commit`], but returns the
    /// [`MutTx`] alongside the `Ok` result of the function `F` without
    /// committing the transaction.
    pub fn with_auto_rollback<F, A, E>(&self, ctx: &ExecutionContext, mut tx: MutTx, f: F) -> Result<(MutTx, A), E>
    where
        F: FnOnce(&mut MutTx) -> Result<A, E>,
    {
        let res = f(&mut tx);
        self.rollback_on_err(ctx, tx, res)
    }

    /// Run a fallible function in a transaction.
    ///
    /// This is similar to `with_auto_commit`, but regardless of the return value of
    /// the fallible function, the transaction will ALWAYS be rolled back. This can be used to
    /// emulate a read-only transaction.
    ///
    /// TODO(jgilles): when we support actual read-only transactions, use those here instead.
    /// TODO(jgilles, kim): get this merged with the above function (two people had similar ideas
    /// at the same time)
    pub fn with_read_only<F, A, E>(&self, ctx: &ExecutionContext, f: F) -> Result<A, E>
    where
        F: FnOnce(&mut Tx) -> Result<A, E>,
        E: From<DBError>,
    {
        let mut tx = self.inner.begin_tx();
        let res = f(&mut tx);
        self.inner.release_tx(ctx, tx);
        res
    }

    /// Perform the transactional logic for the `tx` according to the `res`
    #[tracing::instrument(skip_all)]
    pub fn finish_tx<A, E>(&self, ctx: &ExecutionContext, tx: MutTx, res: Result<A, E>) -> Result<A, E>
    where
        E: From<DBError>,
    {
        if res.is_err() {
            self.rollback_mut_tx(ctx, tx);
        } else {
            match self.commit_tx(ctx, tx).map_err(E::from)? {
                Some(_) => (),
                None => panic!("TODO: retry?"),
            }
        }
        res
    }

    /// Roll back transaction `tx` if `res` is `Err`, otherwise return it
    /// alongside the `Ok` value.
    pub fn rollback_on_err<A, E>(&self, ctx: &ExecutionContext, tx: MutTx, res: Result<A, E>) -> Result<(MutTx, A), E> {
        match res {
            Err(e) => {
                self.rollback_mut_tx(ctx, tx);
                Err(e)
            }
            Ok(a) => Ok((tx, a)),
        }
    }
}

impl RelationalDB {
    pub fn create_table<T: Into<TableDef>>(&self, tx: &mut MutTx, schema: T) -> Result<TableId, DBError> {
        self.inner.create_table_mut_tx(tx, schema.into())
    }

    pub fn drop_table(&self, ctx: &ExecutionContext, tx: &mut MutTx, table_id: TableId) -> Result<(), DBError> {
        #[cfg(feature = "metrics")]
        let _guard = DB_METRICS
            .rdb_drop_table_time
            .with_label_values(&table_id.0)
            .start_timer();
        let table_name = self
            .table_name_from_id(ctx, tx, table_id)?
            .map(|name| name.to_string())
            .unwrap_or_default();
        self.inner.drop_table_mut_tx(tx, table_id).map(|_| {
            DB_METRICS
                .rdb_num_table_rows
                .with_label_values(&self.address, &table_id.into(), &table_name)
                .set(0)
        })
    }

    /// Rename a table.
    ///
    /// Sets the name of the table to `new_name` regardless of the previous value. This is a
    /// relatively cheap operation which only modifies the system tables.
    ///
    /// If the table is not found or is a system table, an error is returned.
    pub fn rename_table(&self, tx: &mut MutTx, table_id: TableId, new_name: &str) -> Result<(), DBError> {
        self.inner.rename_table_mut_tx(tx, table_id, new_name)
    }

    #[tracing::instrument(skip_all)]
    pub fn table_id_from_name_mut(&self, tx: &MutTx, table_name: &str) -> Result<Option<TableId>, DBError> {
        self.inner.table_id_from_name_mut_tx(tx, table_name)
    }

    #[tracing::instrument(skip_all)]
    pub fn table_id_from_name(&self, tx: &Tx, table_name: &str) -> Result<Option<TableId>, DBError> {
        self.inner.table_id_from_name_tx(tx, table_name)
    }

    #[tracing::instrument(skip_all)]
    pub fn table_id_exists(&self, tx: &Tx, table_id: &TableId) -> bool {
        self.inner.table_id_exists_tx(tx, table_id)
    }

    #[tracing::instrument(skip_all)]
    pub fn table_id_exists_mut(&self, tx: &MutTx, table_id: &TableId) -> bool {
        self.inner.table_id_exists_mut_tx(tx, table_id)
    }

    #[tracing::instrument(skip_all)]
    pub fn table_name_from_id<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a MutTx,
        table_id: TableId,
    ) -> Result<Option<Cow<'a, str>>, DBError> {
        self.inner.table_name_from_id_mut_tx(ctx, tx, table_id)
    }

    #[tracing::instrument(skip_all)]
    pub fn column_constraints(
        &self,
        tx: &mut MutTx,
        table_id: TableId,
        cols: &ColList,
    ) -> Result<Constraints, DBError> {
        let table = self.inner.schema_for_table_mut_tx(tx, table_id)?;

        let unique_index = table.indexes.iter().find(|x| &x.columns == cols).map(|x| x.is_unique);
        let attr = Constraints::unset();

        if let Some(is_unique) = unique_index {
            attr.push(if is_unique {
                Constraints::unique()
            } else {
                Constraints::indexed()
            });
        }
        Ok(attr)
    }

    #[tracing::instrument(skip_all)]
    pub fn index_id_from_name(&self, tx: &MutTx, index_name: &str) -> Result<Option<IndexId>, DBError> {
        self.inner.index_id_from_name_mut_tx(tx, index_name)
    }

    #[tracing::instrument(skip_all)]
    pub fn sequence_id_from_name(&self, tx: &MutTx, sequence_name: &str) -> Result<Option<SequenceId>, DBError> {
        self.inner.sequence_id_from_name_mut_tx(tx, sequence_name)
    }

    #[tracing::instrument(skip_all)]
    pub fn constraint_id_from_name(&self, tx: &MutTx, constraint_name: &str) -> Result<Option<ConstraintId>, DBError> {
        self.inner.constraint_id_from_name(tx, constraint_name)
    }

    /// Adds the [index::BTreeIndex] into the [ST_INDEXES_NAME] table
    ///
    /// Returns the `index_id`
    ///
    /// NOTE: It loads the data from the table into it before returning
    #[tracing::instrument(skip(self, tx, index), fields(index=index.index_name))]
    pub fn create_index(&self, tx: &mut MutTx, table_id: TableId, index: IndexDef) -> Result<IndexId, DBError> {
        self.inner.create_index_mut_tx(tx, table_id, index)
    }

    /// Removes the [index::BTreeIndex] from the database by their `index_id`
    #[tracing::instrument(skip(self, tx))]
    pub fn drop_index(&self, tx: &mut MutTx, index_id: IndexId) -> Result<(), DBError> {
        self.inner.drop_index_mut_tx(tx, index_id)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`.
    #[tracing::instrument(skip(self, ctx, tx))]
    pub fn iter_mut<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a MutTx,
        table_id: TableId,
    ) -> Result<Iter<'a>, DBError> {
        self.inner.iter_mut_tx(ctx, tx, table_id)
    }

    #[tracing::instrument(skip(self, ctx, tx))]
    pub fn iter<'a>(&'a self, ctx: &'a ExecutionContext, tx: &'a Tx, table_id: TableId) -> Result<Iter<'a>, DBError> {
        self.inner.iter_tx(ctx, tx, table_id)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `cols` matches `value`.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    #[tracing::instrument(skip_all)]
    pub fn iter_by_col_eq_mut<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a MutTx,
        table_id: impl Into<TableId>,
        cols: impl Into<ColList>,
        value: AlgebraicValue,
    ) -> Result<IterByColEq<'a>, DBError> {
        self.inner.iter_by_col_eq_mut_tx(ctx, tx, table_id.into(), cols, value)
    }

    #[tracing::instrument(skip_all)]
    pub fn iter_by_col_eq<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Tx,
        table_id: impl Into<TableId>,
        cols: impl Into<ColList>,
        value: AlgebraicValue,
    ) -> Result<IterByColEq<'a>, DBError> {
        self.inner.iter_by_col_eq_tx(ctx, tx, table_id.into(), cols, value)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `cols` matches what is within `range`.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub fn iter_by_col_range_mut<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a MutTx,
        table_id: impl Into<TableId>,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<IterByColRange<'a, R>, DBError> {
        self.inner
            .iter_by_col_range_mut_tx(ctx, tx, table_id.into(), cols, range)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `cols` matches what is within `range`.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Tx,
        table_id: impl Into<TableId>,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<IterByColRange<'a, R>, DBError> {
        self.inner.iter_by_col_range_tx(ctx, tx, table_id.into(), cols, range)
    }

    #[tracing::instrument(skip(self, tx, row))]
    pub fn insert(&self, tx: &mut MutTx, table_id: TableId, row: ProductValue) -> Result<ProductValue, DBError> {
        #[cfg(feature = "metrics")]
        let _guard = DB_METRICS
            .rdb_insert_row_time
            .with_label_values(&table_id.0)
            .start_timer();
        self.inner.insert_mut_tx(tx, table_id, row)
    }

    #[tracing::instrument(skip_all)]
    pub fn insert_bytes_as_row(
        &self,
        tx: &mut MutTx,
        table_id: TableId,
        row_bytes: &[u8],
    ) -> Result<ProductValue, DBError> {
        let ty = self.inner.row_type_for_table_mut_tx(tx, table_id)?;
        let row = ProductValue::decode(&ty, &mut &row_bytes[..])?;
        self.insert(tx, table_id, row)
    }

    pub fn delete(&self, tx: &mut MutTx, table_id: TableId, row_ids: impl IntoIterator<Item = RowPointer>) -> u32 {
        self.inner.delete_mut_tx(tx, table_id, row_ids)
    }

    #[tracing::instrument(skip_all)]
    pub fn delete_by_rel<R: Relation>(&self, tx: &mut MutTx, table_id: TableId, relation: R) -> u32 {
        #[cfg(feature = "metrics")]
        let _guard = DB_METRICS
            .rdb_delete_by_rel_time
            .with_label_values(&table_id.0)
            .start_timer();

        self.inner.delete_by_rel_mut_tx(tx, table_id, relation)
    }

    /// Clear all rows from a table without dropping it.
    #[tracing::instrument(skip_all)]
    pub fn clear_table(&self, tx: &mut MutTx, table_id: TableId) -> Result<(), DBError> {
        let relation = self
            .iter_mut(&ExecutionContext::internal(self.address), tx, table_id)?
            .map(|row_ref| row_ref.pointer())
            .collect::<Vec<_>>();
        self.delete(tx, table_id, relation);
        Ok(())
    }

    /// Generated the next value for the [SequenceId]
    #[tracing::instrument(skip_all)]
    pub fn next_sequence(&self, tx: &mut MutTx, seq_id: SequenceId) -> Result<i128, DBError> {
        self.inner.get_next_sequence_value_mut_tx(tx, seq_id)
    }

    /// Add a [Sequence] into the database instance, generates a stable [SequenceId] for it that will persist on restart.
    #[tracing::instrument(skip(self, tx, seq), fields(seq=seq.sequence_name))]
    pub fn create_sequence(
        &mut self,
        tx: &mut MutTx,
        table_id: TableId,
        seq: SequenceDef,
    ) -> Result<SequenceId, DBError> {
        self.inner.create_sequence_mut_tx(tx, table_id, seq)
    }

    ///Removes the [Sequence] from database instance
    #[tracing::instrument(skip(self, tx))]
    pub fn drop_sequence(&self, tx: &mut MutTx, seq_id: SequenceId) -> Result<(), DBError> {
        self.inner.drop_sequence_mut_tx(tx, seq_id)
    }

    ///Removes the [Constraints] from database instance
    #[tracing::instrument(skip(self, tx))]
    pub fn drop_constraint(&self, tx: &mut MutTx, constraint_id: ConstraintId) -> Result<(), DBError> {
        self.inner.drop_constraint_mut_tx(tx, constraint_id)
    }

    /// Retrieve the [`Hash`] of the program (SpacetimeDB module) currently
    /// associated with the database.
    ///
    /// A `None` result indicates that the database is not fully initialized
    /// yet.
    pub fn program_hash(&self, tx: &Tx) -> Result<Option<Hash>, DBError> {
        self.inner.program_hash(tx)
    }

    /// Update the [`Hash`] of the program (SpacetimeDB module) currently
    /// associated with the database.
    ///
    /// The operation runs within the transactional context `tx`.
    ///
    /// The fencing token `fence` must be greater than in any previous
    /// invocations of this method, and is typically obtained from a locking
    /// service.
    ///
    /// The method **MUST** be called within the transaction context which
    /// ensures that any lifecycle reducers (`init`, `update`) are invoked. That
    /// is, an impl of [`crate::host::ModuleInstance`].
    pub(crate) fn set_program_hash(&self, tx: &mut MutTx, fence: u128, hash: Hash) -> Result<(), DBError> {
        self.inner.set_program_hash(tx, fence, hash)
    }
}

fn make_default_ostorage(in_memory: bool, path: impl AsRef<Path>) -> Result<Box<dyn ObjectDB + Send>, DBError> {
    Ok(if in_memory {
        Box::<MemoryObjectDB>::default()
    } else {
        Box::new(HashMapObjectDB::open(path)?)
    })
}

pub fn open_db(path: impl AsRef<Path>, in_memory: bool, fsync: bool) -> Result<RelationalDB, DBError> {
    let path = path.as_ref();
    let mlog = if in_memory {
        None
    } else {
        Some(Arc::new(Mutex::new(
            MessageLog::open(path.join("mlog")).map_err(|e| DBError::Other(e.into()))?,
        )))
    };
    let odb = Arc::new(Mutex::new(make_default_ostorage(in_memory, path.join("odb"))?));
    let stdb = RelationalDB::open(path, mlog, odb, Address::zero(), fsync)?;

    Ok(stdb)
}

pub fn open_log(path: impl AsRef<Path>) -> Result<Arc<Mutex<MessageLog>>, DBError> {
    let path = path.as_ref().to_path_buf();
    Ok(Arc::new(Mutex::new(
        MessageLog::open(path.join("mlog")).map_err(|e| DBError::Other(e.into()))?,
    )))
}

#[cfg(test)]
pub(crate) mod tests_utils {
    use super::*;
    use tempfile::TempDir;

    // Utility for creating a database on a TempDir
    pub(crate) fn make_test_db() -> Result<(RelationalDB, TempDir), DBError> {
        let tmp_dir = TempDir::with_prefix("stdb_test")?;
        let in_memory = false;
        let fsync = false;
        let stdb = open_db(&tmp_dir, in_memory, fsync)?.with_row_count(Arc::new(|_, _| i64::MAX));
        Ok((stdb, tmp_dir))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_macros)]

    use super::*;
    use crate::db::datastore::system_tables::{
        StConstraintRow, StIndexRow, StSequenceRow, StTableRow, ST_CONSTRAINTS_ID, ST_INDEXES_ID, ST_SEQUENCES_ID,
        ST_TABLES_ID,
    };
    use crate::db::message_log::SegmentView;
    use crate::db::ostorage::sled_object_db::SledObjectDB;
    use crate::db::relational_db::tests_utils::make_test_db;
    use crate::error::IndexError;
    use crate::error::LogReplayError;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_sats::db::def::{ColumnDef, ConstraintDef, IndexType};
    use spacetimedb_sats::product;
    use std::io::{self, Seek, SeekFrom, Write};
    use std::ops::Range;
    use tempfile::TempDir;

    fn column(name: &str, ty: AlgebraicType) -> ColumnDef {
        ColumnDef {
            col_name: name.to_string(),
            col_type: ty,
        }
    }

    fn index(name: &str, cols: &[u32]) -> IndexDef {
        IndexDef::btree(
            name.into(),
            cols.iter()
                .copied()
                .map(ColId)
                .collect::<ColListBuilder>()
                .build()
                .unwrap(),
            false,
        )
    }

    fn table(name: &str, columns: Vec<ColumnDef>, indexes: Vec<IndexDef>, constraints: Vec<ConstraintDef>) -> TableDef {
        TableDef::new(name.into(), columns)
            .with_indexes(indexes)
            .with_constraints(constraints)
    }

    #[test]
    fn test() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();
        let schema = TableDef::from_product("MyTable", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        stdb.create_table(&mut tx, schema)?;
        stdb.commit_tx(&ExecutionContext::default(), tx)?;

        Ok(())
    }

    #[test]
    fn test_open_twice() -> ResultTest<()> {
        let (stdb, tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();

        let schema = TableDef::from_product("MyTable", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        stdb.create_table(&mut tx, schema)?;

        stdb.commit_tx(&ExecutionContext::default(), tx)?;

        let mlog = Some(Arc::new(Mutex::new(MessageLog::open(tmp_dir.path().join("mlog"))?)));
        let in_memory = false;
        let odb = Arc::new(Mutex::new(make_default_ostorage(
            in_memory,
            tmp_dir.path().join("odb"),
        )?));

        match RelationalDB::open(tmp_dir.path(), mlog, odb, Address::zero(), true) {
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

        let mut tx = stdb.begin_mut_tx();
        let schema = TableDef::from_product("MyTable", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        let table_id = stdb.create_table(&mut tx, schema)?;
        let t_id = stdb.table_id_from_name_mut(&tx, "MyTable")?;
        assert_eq!(t_id, Some(table_id));
        Ok(())
    }

    #[test]
    fn test_column_name() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();
        let schema = TableDef::from_product("MyTable", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        stdb.create_table(&mut tx, schema)?;
        let table_id = stdb.table_id_from_name_mut(&tx, "MyTable")?.unwrap();
        let schema = stdb.schema_for_table_mut(&tx, table_id)?;
        let col = schema.columns().iter().find(|x| x.col_name == "my_col").unwrap();
        assert_eq!(col.col_pos, 0.into());
        Ok(())
    }

    #[test]
    fn test_create_table_pre_commit() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();
        let schema = TableDef::from_product("MyTable", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        stdb.create_table(&mut tx, schema.clone())?;
        let result = stdb.create_table(&mut tx, schema);
        result.expect_err("create_table should error when called twice");
        Ok(())
    }

    #[test]
    fn test_pre_commit() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();

        let schema = TableDef::from_product("MyTable", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        let table_id = stdb.create_table(&mut tx, schema)?;

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(-1)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(1)])?;

        let mut rows = stdb
            .iter_mut(&ExecutionContext::default(), &tx, table_id)?
            .map(|r| *r.to_product_value().elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![-1, 0, 1]);
        Ok(())
    }

    #[test]
    fn test_post_commit() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();

        let schema = TableDef::from_product("MyTable", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        let table_id = stdb.create_table(&mut tx, schema)?;

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(-1)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(1)])?;
        stdb.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = stdb.begin_mut_tx();
        let mut rows = stdb
            .iter_mut(&ExecutionContext::default(), &tx, table_id)?
            .map(|r| *r.to_product_value().elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![-1, 0, 1]);
        Ok(())
    }

    #[test]
    fn test_filter_range_pre_commit() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();

        let schema = TableDef::from_product("MyTable", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        let table_id = stdb.create_table(&mut tx, schema)?;

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(-1)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(1)])?;

        let mut rows = stdb
            .iter_by_col_range_mut(
                &ExecutionContext::default(),
                &tx,
                table_id,
                ColId(0),
                AlgebraicValue::I32(0)..,
            )?
            .map(|r| *r.to_product_value().elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn test_filter_range_post_commit() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();

        let schema = TableDef::from_product("MyTable", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        let table_id = stdb.create_table(&mut tx, schema)?;

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(-1)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(1)])?;
        stdb.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = stdb.begin_mut_tx();
        let mut rows = stdb
            .iter_by_col_range_mut(
                &ExecutionContext::default(),
                &tx,
                table_id,
                ColId(0),
                AlgebraicValue::I32(0)..,
            )?
            .map(|r| *r.to_product_value().elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        assert_eq!(rows, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn test_create_table_rollback() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();

        let schema = TableDef::from_product("MyTable", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        let table_id = stdb.create_table(&mut tx, schema)?;
        stdb.rollback_mut_tx(&ExecutionContext::default(), tx);

        let tx = stdb.begin_mut_tx();
        let result = stdb.table_id_from_name_mut(&tx, "MyTable")?;
        assert!(
            result.is_none(),
            "Table should not exist, so table_id_from_name should return none"
        );

        let ctx = ExecutionContext::default();

        let result = stdb.table_name_from_id(&ctx, &tx, table_id)?;
        assert!(
            result.is_none(),
            "Table should not exist, so table_name_from_id should return none",
        );
        Ok(())
    }

    #[test]
    fn test_rollback() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();
        let ctx = ExecutionContext::default();

        let schema = TableDef::from_product("MyTable", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        let table_id = stdb.create_table(&mut tx, schema)?;
        stdb.commit_tx(&ctx, tx)?;

        let mut tx = stdb.begin_mut_tx();
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(-1)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I32(1)])?;
        stdb.rollback_mut_tx(&ctx, tx);

        let tx = stdb.begin_mut_tx();
        let mut rows = stdb
            .iter_mut(&ctx, &tx, table_id)?
            .map(|r| *r.to_product_value().elements[0].as_i32().unwrap())
            .collect::<Vec<i32>>();
        rows.sort();

        let expected: Vec<i32> = Vec::new();
        assert_eq!(rows, expected);
        Ok(())
    }

    fn table_auto_inc() -> TableDef {
        TableDef::new(
            "MyTable".into(),
            vec![ColumnDef {
                col_name: "my_col".to_string(),
                col_type: AlgebraicType::I64,
            }],
        )
        .with_column_constraint(Constraints::primary_key_auto(), ColId(0))
    }

    #[test]
    fn test_auto_inc() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();
        let schema = table_auto_inc();
        let table_id = stdb.create_table(&mut tx, schema)?;

        let sequence = stdb.sequence_id_from_name(&tx, "seq_MyTable_my_col_primary_key_auto")?;
        assert!(sequence.is_some(), "Sequence not created");

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(0)])?;

        let mut rows = stdb
            .iter_by_col_range_mut(
                &ExecutionContext::default(),
                &tx,
                table_id,
                ColId(0),
                AlgebraicValue::I64(0)..,
            )?
            .map(|r| *r.to_product_value().elements[0].as_i64().unwrap())
            .collect::<Vec<i64>>();
        rows.sort();

        assert_eq!(rows, vec![1, 2]);

        Ok(())
    }

    #[test]
    fn test_auto_inc_disable() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();
        let schema = table_auto_inc();
        let table_id = stdb.create_table(&mut tx, schema)?;

        let sequence = stdb.sequence_id_from_name(&tx, "seq_MyTable_my_col_primary_key_auto")?;
        assert!(sequence.is_some(), "Sequence not created");

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(5)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(6)])?;

        let mut rows = stdb
            .iter_by_col_range_mut(
                &ExecutionContext::default(),
                &tx,
                table_id,
                ColId(0),
                AlgebraicValue::I64(0)..,
            )?
            .map(|r| *r.to_product_value().elements[0].as_i64().unwrap())
            .collect::<Vec<i64>>();
        rows.sort();

        assert_eq!(rows, vec![5, 6]);

        Ok(())
    }

    fn table_indexed(is_unique: bool) -> TableDef {
        TableDef::new(
            "MyTable".into(),
            vec![ColumnDef {
                col_name: "my_col".to_string(),
                col_type: AlgebraicType::I64,
            }],
        )
        .with_indexes(vec![IndexDef {
            columns: ColList::new(0.into()),
            index_name: "MyTable_my_col_idx".to_string(),
            is_unique,
            index_type: IndexType::BTree,
        }])
    }

    #[test]
    fn test_auto_inc_reload() -> ResultTest<()> {
        let (stdb, tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();
        let schema = TableDef::new(
            "MyTable".into(),
            vec![ColumnDef {
                col_name: "my_col".to_string(),
                col_type: AlgebraicType::I64,
            }],
        )
        .with_column_sequence(ColId(0));

        let table_id = stdb.create_table(&mut tx, schema)?;

        let sequence = stdb.sequence_id_from_name(&tx, "seq_MyTable_my_col")?;
        assert!(sequence.is_some(), "Sequence not created");

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(0)])?;

        let mut rows = stdb
            .iter_by_col_range_mut(
                &ExecutionContext::default(),
                &tx,
                table_id,
                ColId(0),
                AlgebraicValue::I64(0)..,
            )?
            .map(|r| *r.to_product_value().elements[0].as_i64().unwrap())
            .collect::<Vec<i64>>();
        rows.sort();

        assert_eq!(rows, vec![1]);

        stdb.commit_tx(&ExecutionContext::default(), tx)?;
        drop(stdb);

        dbg!("reopen...");
        let stdb = open_db(&tmp_dir, false, true)?;

        let mut tx = stdb.begin_mut_tx();

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(0)])?;

        let mut rows = stdb
            .iter_by_col_range_mut(
                &ExecutionContext::default(),
                &tx,
                table_id,
                ColId(0),
                AlgebraicValue::I64(0)..,
            )?
            .map(|r| *r.to_product_value().elements[0].as_i64().unwrap())
            .collect::<Vec<i64>>();
        rows.sort();

        // Check the second row start after `SEQUENCE_PREALLOCATION_AMOUNT`
        assert_eq!(rows, vec![1, 4098]);
        Ok(())
    }

    #[test]
    fn test_indexed() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();
        let schema = table_indexed(false);
        let table_id = stdb.create_table(&mut tx, schema)?;

        assert!(
            stdb.index_id_from_name(&tx, "MyTable_my_col_idx")?.is_some(),
            "Index not created"
        );

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(1)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(1)])?;

        let mut rows = stdb
            .iter_by_col_range_mut(
                &ExecutionContext::default(),
                &tx,
                table_id,
                ColId(0),
                AlgebraicValue::I64(0)..,
            )?
            .map(|r| *r.to_product_value().elements[0].as_i64().unwrap())
            .collect::<Vec<i64>>();
        rows.sort();

        assert_eq!(rows, vec![1]);

        Ok(())
    }

    #[test]
    fn test_unique() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();

        let schema = table_indexed(true);
        let table_id = stdb.create_table(&mut tx, schema).expect("stdb.create_table failed");

        assert!(
            stdb.index_id_from_name(&tx, "MyTable_my_col_idx")
                .expect("index_id_from_name failed")
                .is_some(),
            "Index not created"
        );

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(1)])
            .expect("stdb.insert failed");
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

        let mut tx = stdb.begin_mut_tx();
        let schema = TableDef::new(
            "MyTable".into(),
            vec![ColumnDef {
                col_name: "my_col".to_string(),
                col_type: AlgebraicType::I64,
            }],
        )
        .with_indexes(vec![IndexDef {
            columns: ColList::new(0.into()),
            index_name: "MyTable_my_col_idx".to_string(),
            is_unique: true,
            index_type: IndexType::BTree,
        }])
        .with_column_constraint(Constraints::identity(), ColId(0));

        let table_id = stdb.create_table(&mut tx, schema)?;

        assert!(
            stdb.index_id_from_name(&tx, "MyTable_my_col_idx")?.is_some(),
            "Index not created"
        );

        let sequence = stdb.sequence_id_from_name(&tx, "seq_MyTable_my_col_identity")?;
        assert!(sequence.is_some(), "Sequence not created");

        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(0)])?;
        stdb.insert(&mut tx, table_id, product![AlgebraicValue::I64(0)])?;

        let mut rows = stdb
            .iter_by_col_range_mut(
                &ExecutionContext::default(),
                &tx,
                table_id,
                ColId(0),
                AlgebraicValue::I64(0)..,
            )?
            .map(|r| *r.to_product_value().elements[0].as_i64().unwrap())
            .collect::<Vec<i64>>();
        rows.sort();

        assert_eq!(rows, vec![1, 2]);

        Ok(())
    }

    #[test]
    fn test_cascade_drop_table() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();
        let schema = TableDef::new(
            "MyTable".into(),
            vec![
                ColumnDef {
                    col_name: "col1".to_string(),
                    col_type: AlgebraicType::I64,
                },
                ColumnDef {
                    col_name: "col2".to_string(),
                    col_type: AlgebraicType::I64,
                },
                ColumnDef {
                    col_name: "col3".to_string(),
                    col_type: AlgebraicType::I64,
                },
                ColumnDef {
                    col_name: "col4".to_string(),
                    col_type: AlgebraicType::I64,
                },
            ],
        )
        .with_indexes(vec![
            IndexDef::btree("MyTable_col1_idx".into(), ColId(0), true),
            IndexDef::btree("MyTable_col3_idx".into(), ColId(0), false),
            IndexDef::btree("MyTable_col4_idx".into(), ColId(0), true),
        ])
        .with_sequences(vec![SequenceDef::for_column("MyTable", "col1", 0.into())])
        .with_constraints(vec![ConstraintDef::for_column(
            "MyTable",
            "col2",
            Constraints::indexed(),
            ColList::new(1.into()),
        )]);

        let ctx = ExecutionContext::default();
        let table_id = stdb.create_table(&mut tx, schema)?;

        let indexes = stdb
            .iter_mut(&ctx, &tx, ST_INDEXES_ID)?
            .map(|x| StIndexRow::try_from(&x.to_product_value()).unwrap().to_owned())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(indexes.len(), 4, "Wrong number of indexes");

        let sequences = stdb
            .iter_mut(&ctx, &tx, ST_SEQUENCES_ID)?
            .map(|x| StSequenceRow::try_from(&x.to_product_value()).unwrap().to_owned())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(sequences.len(), 1, "Wrong number of sequences");

        let constraints = stdb
            .iter_mut(&ctx, &tx, ST_CONSTRAINTS_ID)?
            .map(|x| StConstraintRow::try_from(&x.to_product_value()).unwrap().to_owned())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(constraints.len(), 4, "Wrong number of constraints");

        stdb.drop_table(&ctx, &mut tx, table_id)?;

        let indexes = stdb
            .iter_mut(&ctx, &tx, ST_INDEXES_ID)?
            .map(|x| StIndexRow::try_from(&x.to_product_value()).unwrap().to_owned())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(indexes.len(), 0, "Wrong number of indexes DROP");

        let sequences = stdb
            .iter_mut(&ctx, &tx, ST_SEQUENCES_ID)?
            .map(|x| StSequenceRow::try_from(&x.to_product_value()).unwrap().to_owned())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(sequences.len(), 0, "Wrong number of sequences DROP");

        let constraints = stdb
            .iter_mut(&ctx, &tx, ST_CONSTRAINTS_ID)?
            .map(|x| StConstraintRow::try_from(&x.to_product_value()).unwrap().to_owned())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(constraints.len(), 0, "Wrong number of constraints DROP");

        Ok(())
    }

    #[test]
    fn test_rename_table() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let mut tx = stdb.begin_mut_tx();
        let ctx = ExecutionContext::default();

        let schema = TableDef::new(
            "MyTable".into(),
            vec![ColumnDef {
                col_name: "my_col".to_string(),
                col_type: AlgebraicType::I64,
            }],
        )
        .with_indexes(vec![IndexDef {
            columns: ColList::new(0.into()),
            index_name: "MyTable_my_col_idx".to_string(),
            is_unique: true,
            index_type: IndexType::BTree,
        }]);

        let table_id = stdb.create_table(&mut tx, schema)?;
        stdb.rename_table(&mut tx, table_id, "YourTable")?;
        let table_name = stdb.table_name_from_id(&ctx, &tx, table_id)?;

        assert_eq!(Some("YourTable"), table_name.as_ref().map(Cow::as_ref));
        // Also make sure we've removed the old ST_TABLES_ID row
        let mut n = 0;
        for row in stdb.iter_mut(&ctx, &tx, ST_TABLES_ID)? {
            let row = row.to_product_value();
            let table = StTableRow::try_from(&row)?;
            if table.table_id == table_id {
                n += 1;
            }
        }
        assert_eq!(1, n);

        Ok(())
    }

    #[test]
    fn test_multi_column_index() -> ResultTest<()> {
        let (stdb, _tmp_dir) = make_test_db()?;

        let columns = vec![
            column("a", AlgebraicType::U64),
            column("b", AlgebraicType::U64),
            column("c", AlgebraicType::U64),
        ];

        let indexes = vec![index("0", &[0, 1])];
        let schema = table("t", columns, indexes, vec![]);

        let mut tx = stdb.begin_mut_tx();
        let table_id = stdb.create_table(&mut tx, schema)?;

        stdb.insert(
            &mut tx,
            table_id,
            product![AlgebraicValue::U64(0), AlgebraicValue::U64(0), AlgebraicValue::U64(1)],
        )?;
        stdb.insert(
            &mut tx,
            table_id,
            product![AlgebraicValue::U64(0), AlgebraicValue::U64(1), AlgebraicValue::U64(2)],
        )?;
        stdb.insert(
            &mut tx,
            table_id,
            product![AlgebraicValue::U64(1), AlgebraicValue::U64(2), AlgebraicValue::U64(2)],
        )?;

        let cols = col_list![0, 1];
        let value: AlgebraicValue = product![AlgebraicValue::U64(0), AlgebraicValue::U64(1)].into();

        let ctx = ExecutionContext::default();

        let IterByColEq::Index(mut iter) = stdb.iter_by_col_eq_mut(&ctx, &tx, table_id, cols, value)? else {
            panic!("expected index iterator");
        };

        let Some(row) = iter.next() else {
            panic!("expected non-empty iterator");
        };

        assert_eq!(
            row.to_product_value(),
            product![AlgebraicValue::U64(0), AlgebraicValue::U64(1), AlgebraicValue::U64(2)]
        );

        // iter should only return a single row, so this count should now be 0.
        assert_eq!(iter.count(), 0);
        Ok(())
    }

    // #[test]
    // fn test_rename_column() -> ResultTest<()> {
    //     let (mut stdb, _tmp_dir) = make_test_db()?;

    //     let mut tx_ = stdb.begin_mut_tx();
    //     let (tx, stdb) = tx_.get();

    //     let schema = &[("col1", AlgebraicType::U64, ColumnIndexAttribute::Identity)];
    //     let table_id = stdb.create_table(tx, "MyTable", ProductTypeMeta::from_iter(&schema[..1]))?;
    //     let column_id = stdb.column_id_from_name(tx, table_id, "col1")?.unwrap();
    //     stdb.rename_column(tx, table_id, column_id, "id")?;

    //     assert_eq!(Some(column_id), stdb.column_id_from_name(tx, table_id, "id")?);
    //     assert_eq!(None, stdb.column_id_from_name(tx, table_id, "col1")?);

    //     Ok(())
    // }

    #[test]
    fn test_replay_corrupted_log() -> ResultTest<()> {
        let tmp = TempDir::with_prefix("stdb_test")?;
        let mlog_path = tmp.path().join("mlog");

        const NUM_TRANSACTIONS: usize = 10_000;
        // 64KiB should create like 11 segments
        const MAX_SEGMENT_SIZE: u64 = 64 * 1024;

        let mlog = MessageLog::options()
            .max_segment_size(MAX_SEGMENT_SIZE)
            .open(&mlog_path)
            .map(Mutex::new)
            .map(Arc::new)?;
        let odb = SledObjectDB::open(tmp.path().join("odb"))
            .map(|odb| Box::new(odb) as Box<dyn ObjectDB + Send>)
            .map(Mutex::new)
            .map(Arc::new)?;
        let reopen_db = || RelationalDB::open(tmp.path(), Some(mlog.clone()), odb.clone(), Address::zero(), false);
        let db = reopen_db()?;
        let ctx = ExecutionContext::default();

        let table_id = db.with_auto_commit(&ctx, |tx| {
            db.create_table(
                tx,
                table(
                    "Account",
                    vec![ColumnDef {
                        ..column("deposit", AlgebraicType::U64)
                    }],
                    vec![],
                    vec![ConstraintDef::for_column(
                        "Account",
                        "deposit",
                        Constraints::identity(),
                        ColList::new(0.into()),
                    )],
                ),
            )
        })?;

        fn balance(ctx: &ExecutionContext, db: &RelationalDB, table_id: TableId) -> ResultTest<u64> {
            let balance = db.with_auto_commit(ctx, |tx| -> ResultTest<u64> {
                let last = db
                    .iter_mut(ctx, tx, table_id)?
                    .last()
                    .map(|row| row.to_product_value().field_as_u64(0, None))
                    .transpose()?
                    .unwrap_or_default();
                Ok(last)
            })?;

            Ok(balance)
        }

        // Invalidate a segment by shrinking the file by one byte.
        fn invalidate_shrink(mlog_path: &Path, segment: SegmentView) -> io::Result<()> {
            let segment_file = File::options().write(true).open(
                mlog_path
                    .join(format!("{:0>20}", segment.offset()))
                    .with_extension("log"),
            )?;
            let len = segment_file.metadata()?.len();
            eprintln!("shrink segment segment={segment:?} len={len}");
            segment_file.set_len(len - 1)?;
            segment_file.sync_all()
        }

        // Invalidate a segment by overwriting some portion of the file.
        fn invalidate_overwrite(mlog_path: &Path, segment: SegmentView) -> io::Result<()> {
            let mut segment_file = File::options().write(true).open(
                mlog_path
                    .join(format!("{:0>20}", segment.offset()))
                    .with_extension("log"),
            )?;

            let len = segment_file.metadata()?.len();
            let ofs = len / 2;
            eprintln!("overwrite segment={segment:?} len={len} ofs={ofs}");
            segment_file.seek(SeekFrom::Start(ofs))?;
            segment_file.write_all(&[255, 255, 255, 255])?;
            segment_file.sync_all()
        }

        // Create transactions.
        for _ in 0..NUM_TRANSACTIONS {
            db.with_auto_commit(&ctx, |tx| db.insert(tx, table_id, product![AlgebraicValue::U64(0)]))?;
        }
        assert_eq!(NUM_TRANSACTIONS as u64, balance(&ctx, &db, table_id)?);

        drop(db);
        odb.lock().unwrap().sync_all()?;
        mlog.lock().unwrap().sync_all()?;

        // The state must be the same after reopening the db.
        let db = reopen_db()?;
        assert_eq!(
            NUM_TRANSACTIONS as u64,
            balance(&ctx, &db, table_id)?,
            "the state should be the same as before reopening the db"
        );

        let total_segments = mlog.lock().unwrap().total_segments();
        assert!(total_segments > 3, "expected more than 3 segments");

        // Close the db and pop a byte from the end of the message log.
        drop(db);
        let last_segment = mlog.lock().unwrap().segments().last().unwrap();
        invalidate_shrink(&mlog_path, last_segment.clone())?;

        // Assert that the final tx is lost.
        let db = reopen_db()?;
        assert_eq!(
            (NUM_TRANSACTIONS - 1) as u64,
            balance(&ctx, &db, table_id)?,
            "the last transaction should have been dropped"
        );
        assert_eq!(
            total_segments,
            mlog.lock().unwrap().total_segments(),
            "no segment should have beeen removed"
        );

        // Overwrite some portion of the last segment.
        drop(db);
        let last_segment = mlog.lock().unwrap().segments().last().unwrap();
        invalidate_overwrite(&mlog_path, last_segment)?;
        let res = reopen_db();
        if !matches!(res, Err(DBError::LogReplay(LogReplayError::OutOfOrderCommit { .. }))) {
            panic!("Expected replay error but got: {res:?}");
        }
        // We can't recover from this, so drop the last segment.
        let mut mlog_guard = mlog.lock().unwrap();
        let drop_segment = mlog_guard.segments().last().unwrap();
        mlog_guard.reset_to(drop_segment.offset() - 1)?;
        let last_segment = mlog_guard.segments().last().unwrap();
        drop(mlog_guard);

        let segment_range = Range {
            start: last_segment.offset(),
            end: drop_segment.offset() - 1,
        };
        let db = reopen_db()?;
        let balance = balance(&ctx, &db, table_id)?;
        assert!(
            segment_range.contains(&balance),
            "balance {balance} should fall within {segment_range:?}"
        );
        assert_eq!(
            total_segments - 1,
            mlog.lock().unwrap().total_segments(),
            "one segment should have beeen removed"
        );

        // Now, let's poke a segment somewhere in the middle of the log.
        drop(db);
        let segment = mlog.lock().unwrap().segments().nth(5).unwrap();
        invalidate_shrink(&mlog_path, segment)?;

        let res = reopen_db();
        if !matches!(res, Err(DBError::LogReplay(LogReplayError::TrailingSegments { .. }))) {
            panic!("Expected `LogReplayError::TrailingSegments` but got: {res:?}")
        }

        // The same should happen if we overwrite instead of shrink.
        let segment = mlog.lock().unwrap().segments().nth(5).unwrap();
        invalidate_overwrite(&mlog_path, segment)?;

        let res = reopen_db();
        if !matches!(res, Err(DBError::LogReplay(LogReplayError::OutOfOrderCommit { .. }))) {
            panic!("Expected `LogReplayError::OutOfOrderCommit` but got: {res:?}")
        }

        Ok(())
    }

    #[test]
    /// Test that iteration yields each row only once
    /// in the edge case where a row is committed and has been deleted and re-inserted within the iterating TX.
    fn test_insert_delete_insert_iter() {
        let (stdb, _tmp_dir) = make_test_db().expect("make_test_db failed");
        let ctx = ExecutionContext::default();

        let mut initial_tx = stdb.begin_mut_tx();
        let schema = TableDef::from_product("test_table", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        let table_id = stdb.create_table(&mut initial_tx, schema).expect("create_table failed");

        stdb.commit_tx(&ctx, initial_tx).expect("Commit initial_tx failed");

        // Insert a row and commit it, so the row is in the committed_state.
        let mut insert_tx = stdb.begin_mut_tx();
        stdb.insert(&mut insert_tx, table_id, product!(AlgebraicValue::I32(0)))
            .expect("Insert insert_tx failed");
        stdb.commit_tx(&ctx, insert_tx).expect("Commit insert_tx failed");

        let mut delete_insert_tx = stdb.begin_mut_tx();
        // Delete the row, so it's in the `delete_tables` of `delete_insert_tx`.
        assert_eq!(
            stdb.delete_by_rel(&mut delete_insert_tx, table_id, [product!(AlgebraicValue::I32(0))]),
            1
        );

        // Insert the row again, so that depending on the datastore internals,
        // it may now be only in the committed_state,
        // or in all three of the committed_state, delete_tables and insert_tables.
        stdb.insert(&mut delete_insert_tx, table_id, product!(AlgebraicValue::I32(0)))
            .expect("Insert delete_insert_tx failed");

        // Iterate over the table and assert that we see the committed-deleted-inserted row only once.
        assert_eq!(
            &stdb
                .iter_mut(&ctx, &delete_insert_tx, table_id)
                .expect("iter delete_insert_tx failed")
                .map(|row_ref| row_ref.to_product_value())
                .collect::<Vec<_>>(),
            &[product!(AlgebraicValue::I32(0))],
        );

        stdb.rollback_mut_tx(&ctx, delete_insert_tx);
    }
}
