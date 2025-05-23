use super::scheduler::{get_schedule_from_row, ScheduleError, Scheduler};
use crate::database_logger::{BacktraceProvider, LogLevel, Record};
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::relational_db::{MutTx, RelationalDB};
use crate::error::{DBError, IndexError, NodesError};
use crate::replica_context::ReplicaContext;
use core::mem;
use parking_lot::{Mutex, MutexGuard};
use smallvec::SmallVec;
use spacetimedb_lib::Timestamp;
use spacetimedb_primitives::{ColId, ColList, IndexId, TableId};
use spacetimedb_sats::{
    bsatn::{self, ToBsatn},
    buffer::{CountWriter, TeeWriter},
    AlgebraicValue, ProductValue,
};
use spacetimedb_table::indexes::RowPointer;
use spacetimedb_table::table::RowRef;
use std::ops::DerefMut;
use std::sync::Arc;

#[derive(Clone)]
pub struct InstanceEnv {
    pub replica_ctx: Arc<ReplicaContext>,
    pub scheduler: Scheduler,
    pub tx: TxSlot,
    /// The timestamp the current reducer began running.
    pub start_time: Timestamp,
}

#[derive(Clone, Default)]
pub struct TxSlot {
    inner: Arc<Mutex<Option<MutTxId>>>,
}

/// The maximum number of chunks stored in a single [`ChunkPool`].
///
/// When returning a chunk to the pool via [`ChunkPool::put`],
/// if the pool contains more than [`MAX_CHUNKS_IN_POOL`] chunks,
/// the returned chunk will be freed rather than added to the pool.
///
/// This, together with [`MAX_CHUNK_SIZE_IN_BYTES`],
/// prevents the heap usage of a [`ChunkPool`] from growing without bound.
///
/// This number chosen completely arbitrarily by pgoldman 2025-04-10.
const MAX_CHUNKS_IN_POOL: usize = 32;

/// The maximum size of chunks which can be saved in a [`ChunkPool`].
///
/// When returning a chunk to the pool via [`ChunkPool::put`],
/// if the returned chunk is larger than [`MAX_CHUNK_SIZE_IN_BYTES`],
/// the returned chunk will be freed rather than added to the pool.
///
/// This, together with [`MAX_CHUNKS_IN_POOL`],
/// prevents the heap usage of a [`ChunkPool`] from growing without bound.
///
/// We switch to a new chunk when we pass ROW_ITER_CHUNK_SIZE, so this adds a buffer of 4x.
const MAX_CHUNK_SIZE_IN_BYTES: usize = spacetimedb_primitives::ROW_ITER_CHUNK_SIZE * 4;

/// A pool of available unused chunks.
///
/// The number of chunks stored in a `ChunkPool` is limited by [`MAX_CHUNKS_IN_POOL`],
/// and the size of each individual saved chunk is limited by [`MAX_CHUNK_SIZE_IN_BYTES`].
#[derive(Default)]
pub struct ChunkPool {
    free_chunks: Vec<Vec<u8>>,
}

impl ChunkPool {
    /// Takes an unused chunk from this pool
    /// or creates a new chunk if none are available.
    /// New chunks are not actually allocated,
    /// but will be, on first use.
    fn take(&mut self) -> Vec<u8> {
        self.free_chunks.pop().unwrap_or_default()
    }

    /// Return a chunk back to the pool, or frees it, as appropriate.
    ///
    /// `chunk` will be freed if either:
    ///
    /// - `self` already contains at least [`MAX_CHUNKS_IN_POOL`] chunks, or
    /// - `chunk.capacity()` is greater than [`MAX_CHUNK_SIZE_IN_BYTES`].
    ///
    /// These limits place an upper bound on the memory usage of a single [`ChunkPool`].
    pub fn put(&mut self, mut chunk: Vec<u8>) {
        if chunk.capacity() > MAX_CHUNK_SIZE_IN_BYTES {
            return;
        }
        if self.free_chunks.len() > MAX_CHUNKS_IN_POOL {
            return;
        }
        chunk.clear();
        self.free_chunks.push(chunk);
    }
}

/// Construct a new `ChunkedWriter` using [`Self::new`].
/// Do not impl `Default` for this struct or construct it manually;
/// it is important that all allocated chunks are taken from the [`ChunkPool`],
/// rather than directly from the global allocator.
struct ChunkedWriter {
    /// Chunks collected thus far.
    chunks: Vec<Vec<u8>>,
    /// Current in progress chunk that will be added to `chunks`.
    curr: Vec<u8>,
}

impl ChunkedWriter {
    /// Flushes the data collected in the current chunk
    /// if it's larger than our chunking threshold.
    fn flush(&mut self, pool: &mut ChunkPool) {
        if self.curr.len() > spacetimedb_primitives::ROW_ITER_CHUNK_SIZE {
            let curr = mem::replace(&mut self.curr, pool.take());
            self.chunks.push(curr);
        }
    }

    /// Creates a new `ChunkedWriter` with an empty chunk allocated from the pool.
    fn new(pool: &mut ChunkPool) -> Self {
        Self {
            chunks: Vec::new(),
            curr: pool.take(),
        }
    }

    /// Finalises the writer and returns all the chunks.
    fn into_chunks(mut self) -> Vec<Vec<u8>> {
        if !self.curr.is_empty() {
            self.chunks.push(self.curr);
        }
        self.chunks
    }

    pub fn collect_iter(
        pool: &mut ChunkPool,
        iter: impl Iterator<Item = impl ToBsatn>,
        rows_scanned: &mut usize,
        bytes_scanned: &mut usize,
    ) -> Vec<Vec<u8>> {
        let mut chunked_writer = Self::new(pool);
        // Consume the iterator, serializing each `item`,
        // while allowing a chunk to be created at boundaries.
        for item in iter {
            // Write the item directly to the BSATN `chunked_writer` buffer.
            item.to_bsatn_extend(&mut chunked_writer.curr).unwrap();
            // Flush at item boundaries.
            chunked_writer.flush(pool);
            // Update rows scanned
            *rows_scanned += 1;
        }

        let chunks = chunked_writer.into_chunks();

        // Update (BSATN) bytes scanned
        *bytes_scanned += chunks.iter().map(|chunk| chunk.len()).sum::<usize>();

        chunks
    }
}

// Generic 'instance environment' delegated to from various host types.
impl InstanceEnv {
    pub fn new(replica_ctx: Arc<ReplicaContext>, scheduler: Scheduler) -> Self {
        Self {
            replica_ctx,
            scheduler,
            tx: TxSlot::default(),
            start_time: Timestamp::now(),
        }
    }

    /// Signal to this `InstanceEnv` that a reducer call is beginning.
    pub fn start_reducer(&mut self, ts: Timestamp) {
        self.start_time = ts;
    }

    fn get_tx(&self) -> Result<impl DerefMut<Target = MutTxId> + '_, GetTxError> {
        self.tx.get()
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn console_log(&self, level: LogLevel, record: &Record, bt: &dyn BacktraceProvider) {
        self.replica_ctx.logger.write(level, record, bt);
        log::trace!(
            "MOD({}): {}",
            self.replica_ctx.database_identity.to_abbreviated_hex(),
            record.message
        );
    }

    /// Project `cols` in `row_ref` encoded in BSATN to `buffer`
    /// and return the full length of the BSATN.
    ///
    /// Assumes that the full encoding of `cols` will fit in `buffer`.
    fn project_cols_bsatn(buffer: &mut [u8], cols: ColList, row_ref: RowRef<'_>) -> usize {
        // We get back a col-list with the columns with generated values.
        // Write those back to `buffer` and then the encoded length to `row_len`.
        let counter = CountWriter::default();
        let mut writer = TeeWriter::new(counter, buffer);
        for col in cols.iter() {
            // Read the column value to AV and then serialize.
            let val = row_ref
                .read_col::<AlgebraicValue>(col)
                .expect("reading col as AV never panics");
            bsatn::to_writer(&mut writer, &val).unwrap();
        }
        writer.w1.finish()
    }

    pub fn insert(&self, table_id: TableId, buffer: &mut [u8]) -> Result<usize, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.get_tx()?;

        let (row_len, row_ptr, insert_flags) = stdb
            .insert(tx, table_id, buffer)
            .map(|(gen_cols, row_ref, insert_flags)| {
                let row_len = Self::project_cols_bsatn(buffer, gen_cols, row_ref);
                (row_len, row_ref.pointer(), insert_flags)
            })
            .inspect_err(
                #[cold]
                #[inline(never)]
                |e| match e {
                    DBError::Index(IndexError::UniqueConstraintViolation(_)) => {}
                    _ => {
                        let res = stdb.table_name_from_id_mut(tx, table_id);
                        if let Ok(Some(table_name)) = res {
                            log::debug!("insert(table: {table_name}, table_id: {table_id}): {e}")
                        } else {
                            log::debug!("insert(table_id: {table_id}): {e}")
                        }
                    }
                },
            )?;

        if insert_flags.is_scheduler_table {
            self.schedule_row(stdb, tx, table_id, row_ptr)?;
        }

        // Note, we update the metric for bytes written after the insert.
        // This is to capture auto-inc columns.
        tx.metrics.bytes_written += buffer.len();

        Ok(row_len)
    }

    #[cold]
    #[inline(never)]
    fn schedule_row(
        &self,
        stdb: &RelationalDB,
        tx: &mut MutTx,
        table_id: TableId,
        row_ptr: RowPointer,
    ) -> Result<(), NodesError> {
        let (id_column, at_column) = stdb
            .table_scheduled_id_and_at(tx, table_id)?
            .expect("schedule_row should only be called when we know its a scheduler table");

        let row_ref = tx.get(table_id, row_ptr)?.unwrap();
        let (schedule_id, schedule_at) = get_schedule_from_row(&row_ref, id_column, at_column)
            // NOTE(centril): Should never happen,
            // as we successfully inserted and thus `ret` is verified against the table schema.
            .map_err(|e| NodesError::ScheduleError(ScheduleError::DecodingError(e)))?;
        self.scheduler
            .schedule(
                table_id,
                schedule_id,
                schedule_at,
                id_column,
                at_column,
                self.start_time,
            )
            .map_err(NodesError::ScheduleError)?;

        Ok(())
    }

    pub fn update(&self, table_id: TableId, index_id: IndexId, buffer: &mut [u8]) -> Result<usize, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.get_tx()?;

        let (row_len, row_ptr, update_flags) = stdb
            .update(tx, table_id, index_id, buffer)
            .map(|(gen_cols, row_ref, update_flags)| {
                let row_len = Self::project_cols_bsatn(buffer, gen_cols, row_ref);
                (row_len, row_ref.pointer(), update_flags)
            })
            .inspect_err(
                #[cold]
                #[inline(never)]
                |e| match e {
                    DBError::Index(IndexError::UniqueConstraintViolation(_)) => {}
                    _ => {
                        let res = stdb.table_name_from_id_mut(tx, table_id);
                        if let Ok(Some(table_name)) = res {
                            log::debug!("update(table: {table_name}, table_id: {table_id}, index_id: {index_id}): {e}")
                        } else {
                            log::debug!("update(table_id: {table_id}, index_id: {index_id}): {e}")
                        }
                    }
                },
            )?;

        if update_flags.is_scheduler_table {
            self.schedule_row(stdb, tx, table_id, row_ptr)?;
        }
        tx.metrics.bytes_written += buffer.len();
        tx.metrics.rows_updated += 1;

        Ok(row_len)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn datastore_delete_by_index_scan_range_bsatn(
        &self,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<u32, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.tx.get()?;

        // Find all rows in the table to delete.
        let (table_id, iter) = stdb.index_scan_range(tx, index_id, prefix, prefix_elems, rstart, rend)?;
        // Re. `SmallVec`, `delete_by_field` only cares about 1 element, so optimize for that.
        let rows_to_delete = iter.map(|row_ref| row_ref.pointer()).collect::<SmallVec<[_; 1]>>();

        // Note, we're deleting rows based on the result of a btree scan.
        // Hence we must update our `index_seeks` and `rows_scanned` metrics.
        //
        // Note that we're not updating `bytes_scanned` at all,
        // because we never dereference any of the returned `RowPointer`s.
        tx.metrics.index_seeks += 1;
        tx.metrics.rows_scanned += rows_to_delete.len();

        // Delete them and count how many we deleted.
        Ok(stdb.delete(tx, table_id, rows_to_delete))
    }

    /// Deletes all rows in the table identified by `table_id`
    /// where the rows match one in `relation`
    /// which is a bsatn encoding of `Vec<ProductValue>`.
    ///
    /// Returns an error if
    /// - not in a transaction.
    /// - the table didn't exist.
    /// - a row couldn't be decoded to the table schema type.
    #[tracing::instrument(level = "trace", skip(self, relation))]
    pub fn datastore_delete_all_by_eq_bsatn(&self, table_id: TableId, relation: &[u8]) -> Result<u32, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.get_tx()?;

        // Track the number of bytes coming from the caller
        tx.metrics.bytes_scanned += relation.len();

        // Find the row schema using it to decode a vector of product values.
        let row_ty = stdb.row_schema_for_table(tx, table_id)?;
        // `TableType::delete` cares about a single element
        // so in that case we can avoid the allocation by using `smallvec`.
        let relation = ProductValue::decode_smallvec(&row_ty, &mut &*relation).map_err(NodesError::DecodeRow)?;

        // Note, we track the number of rows coming from the caller,
        // regardless of whether or not we actually delete them,
        // since we have to derive row ids for each one of them.
        tx.metrics.rows_scanned += relation.len();

        // Delete them and return how many we deleted.
        Ok(stdb.delete_by_rel(tx, table_id, relation))
    }

    /// Returns the `table_id` associated with the given `table_name`.
    ///
    /// Errors with `GetTxError` if not in a transaction
    /// and `TableNotFound` if the table does not exist.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn table_id_from_name(&self, table_name: &str) -> Result<TableId, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.get_tx()?;

        // Query the table id from the name.
        stdb.table_id_from_name_mut(tx, table_name)?
            .ok_or(NodesError::TableNotFound)
    }

    /// Returns the `index_id` associated with the given `index_name`.
    ///
    /// Errors with `GetTxError` if not in a transaction
    /// and `IndexNotFound` if the index does not exist.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn index_id_from_name(&self, index_name: &str) -> Result<IndexId, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.get_tx()?;

        // Query the index id from the name.
        stdb.index_id_from_name_mut(tx, index_name)?
            .ok_or(NodesError::IndexNotFound)
    }

    /// Returns the number of rows in the table identified by `table_id`.
    ///
    /// Errors with `GetTxError` if not in a transaction
    /// and `TableNotFound` if the table does not exist.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn datastore_table_row_count(&self, table_id: TableId) -> Result<u64, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.get_tx()?;

        // Query the row count for id.
        stdb.table_row_count_mut(tx, table_id).ok_or(NodesError::TableNotFound)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn datastore_table_scan_bsatn_chunks(
        &self,
        pool: &mut ChunkPool,
        table_id: TableId,
    ) -> Result<Vec<Vec<u8>>, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.tx.get()?;

        // Track the number of rows and the number of bytes scanned by the iterator
        let mut rows_scanned = 0;
        let mut bytes_scanned = 0;

        // Scan table and serialize rows to bsatn
        let chunks = ChunkedWriter::collect_iter(
            pool,
            stdb.iter_mut(tx, table_id)?,
            &mut rows_scanned,
            &mut bytes_scanned,
        );

        tx.metrics.rows_scanned += rows_scanned;
        tx.metrics.bytes_scanned += bytes_scanned;

        Ok(chunks)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn datastore_index_scan_range_bsatn_chunks(
        &self,
        pool: &mut ChunkPool,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<Vec<Vec<u8>>, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.tx.get()?;

        // Track rows and bytes scanned by the iterator
        let mut rows_scanned = 0;
        let mut bytes_scanned = 0;

        // Open index iterator
        let (_, iter) = stdb.index_scan_range(tx, index_id, prefix, prefix_elems, rstart, rend)?;

        // Scan the index and serialize rows to bsatn
        let chunks = ChunkedWriter::collect_iter(pool, iter, &mut rows_scanned, &mut bytes_scanned);

        tx.metrics.index_seeks += 1;
        tx.metrics.rows_scanned += rows_scanned;
        tx.metrics.bytes_scanned += bytes_scanned;

        Ok(chunks)
    }
}

impl TxSlot {
    pub fn set<T>(&mut self, tx: MutTxId, f: impl FnOnce() -> T) -> (MutTxId, T) {
        let prev = self.inner.lock().replace(tx);
        assert!(prev.is_none(), "reentrant TxSlot::set");
        let remove_tx = || self.inner.lock().take();

        let res = {
            scopeguard::defer_on_unwind! { remove_tx(); }
            f()
        };

        let tx = remove_tx().expect("tx was removed during transaction");
        (tx, res)
    }

    pub fn get(&self) -> Result<impl DerefMut<Target = MutTxId> + '_, GetTxError> {
        MutexGuard::try_map(self.inner.lock(), |map| map.as_mut()).map_err(|_| GetTxError)
    }
}

#[derive(Debug)]
pub struct GetTxError;
impl From<GetTxError> for NodesError {
    fn from(_: GetTxError) -> Self {
        NodesError::NotInTransaction
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::{ops::Bound, sync::Arc};

    use crate::{
        database_logger::DatabaseLogger,
        db::relational_db::{
            tests_utils::{begin_mut_tx, with_auto_commit, with_read_only, TestDB},
            RelationalDB,
        },
        host::Scheduler,
        messages::control_db::{Database, HostType},
        replica_context::ReplicaContext,
        subscription::module_subscription_actor::ModuleSubscriptions,
    };
    use anyhow::{anyhow, Result};
    use spacetimedb_lib::db::auth::StAccess;
    use spacetimedb_lib::{bsatn::to_vec, AlgebraicType, AlgebraicValue, Hash, Identity, ProductValue};
    use spacetimedb_paths::{server::ModuleLogsDir, FromPathUnchecked};
    use spacetimedb_primitives::{IndexId, TableId};
    use spacetimedb_sats::product;
    use tempfile::TempDir;

    /// An `InstanceEnv` requires a `DatabaseLogger`
    fn temp_logger() -> Result<DatabaseLogger> {
        let temp = TempDir::new()?;
        let path = ModuleLogsDir::from_path_unchecked(temp.into_path());
        let path = path.today();
        Ok(DatabaseLogger::open(path))
    }

    /// An `InstanceEnv` requires `ModuleSubscriptions`
    fn subscription_actor(relational_db: Arc<RelationalDB>) -> ModuleSubscriptions {
        ModuleSubscriptions::new(relational_db, <_>::default(), Identity::ZERO)
    }

    /// An `InstanceEnv` requires a `ReplicaContext`.
    /// For our purposes this is just a wrapper for `RelationalDB`.
    fn replica_ctx(relational_db: Arc<RelationalDB>) -> Result<ReplicaContext> {
        Ok(ReplicaContext {
            database: Database {
                id: 0,
                database_identity: Identity::ZERO,
                owner_identity: Identity::ZERO,
                host_type: HostType::Wasm,
                initial_program: Hash::ZERO,
            },
            replica_id: 0,
            logger: Arc::new(temp_logger()?),
            subscriptions: subscription_actor(relational_db.clone()),
            relational_db,
        })
    }

    /// An `InstanceEnv` used for testing the database syscalls.
    fn instance_env(db: Arc<RelationalDB>) -> Result<InstanceEnv> {
        let (scheduler, _) = Scheduler::open(db.clone());
        Ok(InstanceEnv {
            replica_ctx: Arc::new(replica_ctx(db)?),
            scheduler,
            tx: TxSlot::default(),
            start_time: Timestamp::now(),
        })
    }

    /// An in-memory `RelationalDB` for testing.
    /// It does not persist data to disk.
    fn relational_db() -> Result<Arc<RelationalDB>> {
        let TestDB { db, .. } = TestDB::in_memory()?;
        Ok(Arc::new(db))
    }

    /// Generate a `ProductValue` for use in [create_table_with_index]
    fn product_row(i: usize) -> ProductValue {
        let str = i.to_string();
        let str = str.repeat(i);
        let id = i as u64;
        product!(id, str)
    }

    /// Generate a BSATN encoded row for use in [create_table_with_index]
    fn bsatn_row(i: usize) -> Result<Vec<u8>> {
        Ok(to_vec(&product_row(i))?)
    }

    /// Instantiate the following table:
    ///
    /// ```text
    /// id | str
    /// -- | ---
    /// 1  | "1"
    /// 2  | "22"
    /// 3  | "333"
    /// 4  | "4444"
    /// 5  | "55555"
    /// ```
    ///
    /// with an index on `id`.
    fn create_table_with_index(db: &RelationalDB) -> Result<(TableId, IndexId)> {
        let table_id = db.create_table_for_test(
            "t",
            &[("id", AlgebraicType::U64), ("str", AlgebraicType::String)],
            &[0.into()],
        )?;
        let index_id = with_read_only(db, |tx| {
            db.schema_for_table(tx, table_id)?
                .indexes
                .iter()
                .find(|schema| {
                    schema
                        .index_algorithm
                        .columns()
                        .as_singleton()
                        .is_some_and(|col_id| col_id.idx() == 0)
                })
                .map(|schema| schema.index_id)
                .ok_or_else(|| anyhow!("Index not found for ColId `{}`", 0))
        })?;
        with_auto_commit(db, |tx| -> Result<_> {
            for i in 1..=5 {
                db.insert(tx, table_id, &bsatn_row(i)?)?;
            }
            Ok(())
        })?;
        Ok((table_id, index_id))
    }

    fn create_table_with_unique_index(db: &RelationalDB) -> Result<(TableId, IndexId)> {
        let table_id = db.create_table_for_test_with_the_works(
            "t",
            &[("id", AlgebraicType::U64), ("str", AlgebraicType::String)],
            &[0.into()],
            &[0.into()],
            StAccess::Public,
        )?;
        let index_id = with_read_only(db, |tx| {
            db.schema_for_table(tx, table_id)?
                .indexes
                .iter()
                .find(|schema| {
                    schema
                        .index_algorithm
                        .columns()
                        .as_singleton()
                        .is_some_and(|col_id| col_id.idx() == 0)
                })
                .map(|schema| schema.index_id)
                .ok_or_else(|| anyhow!("Index not found for ColId `{}`", 0))
        })?;
        with_auto_commit(db, |tx| -> Result<_> {
            for i in 1..=5 {
                db.insert(tx, table_id, &bsatn_row(i)?)?;
            }
            Ok(())
        })?;
        Ok((table_id, index_id))
    }

    #[test]
    fn table_scan_metrics() -> Result<()> {
        let db = relational_db()?;
        let env = instance_env(db.clone())?;

        let (table_id, _) = create_table_with_index(&db)?;

        let mut tx_slot = env.tx.clone();

        let f = || env.datastore_table_scan_bsatn_chunks(&mut ChunkPool::default(), table_id);
        let tx = begin_mut_tx(&db);
        let (tx, scan_result) = tx_slot.set(tx, f);

        scan_result?;

        let bytes_scanned = (1..=5)
            .map(bsatn_row)
            .filter_map(|bsatn_result| bsatn_result.ok())
            .map(|bsatn| bsatn.len())
            .sum::<usize>();

        // The only non-zero metrics should be rows and bytes scanned.
        // The table has 5 rows, so we should have 5 rows scanned.
        // We should also have scanned the same number of bytes that we inserted.
        assert_eq!(0, tx.metrics.index_seeks);
        assert_eq!(5, tx.metrics.rows_scanned);
        assert_eq!(bytes_scanned, tx.metrics.bytes_scanned);
        assert_eq!(0, tx.metrics.bytes_written);
        assert_eq!(0, tx.metrics.bytes_sent_to_clients);
        Ok(())
    }

    #[test]
    fn index_scan_metrics() -> Result<()> {
        let db = relational_db()?;
        let env = instance_env(db.clone())?;

        let (_, index_id) = create_table_with_index(&db)?;

        let mut tx_slot = env.tx.clone();

        // Perform two index scans
        let f = || -> Result<_> {
            let index_key_3 = to_vec(&Bound::Included(AlgebraicValue::U64(3)))?;
            let index_key_5 = to_vec(&Bound::Included(AlgebraicValue::U64(5)))?;
            env.datastore_index_scan_range_bsatn_chunks(
                &mut ChunkPool::default(),
                index_id,
                &[],
                0.into(),
                &index_key_3,
                &index_key_3,
            )?;
            env.datastore_index_scan_range_bsatn_chunks(
                &mut ChunkPool::default(),
                index_id,
                &[],
                0.into(),
                &index_key_5,
                &index_key_5,
            )?;
            Ok(())
        };
        let tx = begin_mut_tx(&db);
        let (tx, scan_result) = tx_slot.set(tx, f);

        scan_result?;

        let bytes_scanned = [3, 5]
            .into_iter()
            .map(bsatn_row)
            .filter_map(|bsatn_result| bsatn_result.ok())
            .map(|bsatn| bsatn.len())
            .sum::<usize>();

        // We performed two index scans to fetch rows 3 and 5
        assert_eq!(2, tx.metrics.index_seeks);
        assert_eq!(2, tx.metrics.rows_scanned);
        assert_eq!(bytes_scanned, tx.metrics.bytes_scanned);
        assert_eq!(0, tx.metrics.bytes_written);
        assert_eq!(0, tx.metrics.bytes_sent_to_clients);
        Ok(())
    }

    #[test]
    fn insert_metrics() -> Result<()> {
        let db = relational_db()?;
        let env = instance_env(db.clone())?;

        let (table_id, _) = create_table_with_index(&db)?;

        let mut tx_slot = env.tx.clone();

        // Insert 4 new rows into `t`
        let f = || -> Result<_> {
            for i in 6..=9 {
                let mut buffer = bsatn_row(i)?;
                env.insert(table_id, &mut buffer)?;
            }
            Ok(())
        };
        let tx = begin_mut_tx(&db);
        let (tx, insert_result) = tx_slot.set(tx, f);

        insert_result?;

        let bytes_written = (6..=9)
            .map(bsatn_row)
            .filter_map(|bsatn_result| bsatn_result.ok())
            .map(|bsatn| bsatn.len())
            .sum::<usize>();

        // The only metric affected by inserts is bytes written
        assert_eq!(0, tx.metrics.index_seeks);
        assert_eq!(0, tx.metrics.rows_scanned);
        assert_eq!(0, tx.metrics.bytes_scanned);
        assert_eq!(bytes_written, tx.metrics.bytes_written);
        assert_eq!(0, tx.metrics.bytes_sent_to_clients);
        Ok(())
    }

    #[test]
    fn update_metrics() -> Result<()> {
        let db = relational_db()?;
        let env = instance_env(db.clone())?;

        let (table_id, index_id) = create_table_with_unique_index(&db)?;

        let mut tx_slot = env.tx.clone();

        let row_id: u64 = 1;
        let row_val: String = "string".to_string();
        let mut new_row_bytes = to_vec(&product!(row_id, row_val))?;
        let new_row_len = new_row_bytes.len();
        // Delete a single row via the index
        let f = || -> Result<_> {
            env.update(table_id, index_id, new_row_bytes.as_mut_slice())?;
            Ok(())
        };
        let tx = begin_mut_tx(&db);
        let (tx, res) = tx_slot.set(tx, f);

        res?;

        assert_eq!(new_row_len, tx.metrics.bytes_written);
        Ok(())
    }

    #[test]
    fn delete_by_index_metrics() -> Result<()> {
        let db = relational_db()?;
        let env = instance_env(db.clone())?;

        let (_, index_id) = create_table_with_index(&db)?;

        let mut tx_slot = env.tx.clone();

        // Delete a single row via the index
        let f = || -> Result<_> {
            let index_key = to_vec(&Bound::Included(AlgebraicValue::U64(3)))?;
            env.datastore_delete_by_index_scan_range_bsatn(index_id, &[], 0.into(), &index_key, &index_key)?;
            Ok(())
        };
        let tx = begin_mut_tx(&db);
        let (tx, delete_result) = tx_slot.set(tx, f);

        delete_result?;

        assert_eq!(1, tx.metrics.index_seeks);
        assert_eq!(1, tx.metrics.rows_scanned);
        assert_eq!(0, tx.metrics.bytes_scanned);
        assert_eq!(0, tx.metrics.bytes_written);
        assert_eq!(0, tx.metrics.bytes_sent_to_clients);
        Ok(())
    }

    #[test]
    fn delete_by_value_metrics() -> Result<()> {
        let db = relational_db()?;
        let env = instance_env(db.clone())?;

        let (table_id, _) = create_table_with_index(&db)?;

        let mut tx_slot = env.tx.clone();

        let bsatn_rows = to_vec(&(3..=5).map(product_row).collect::<Vec<_>>())?;

        // Delete 3 rows by value
        let f = || -> Result<_> {
            env.datastore_delete_all_by_eq_bsatn(table_id, &bsatn_rows)?;
            Ok(())
        };
        let tx = begin_mut_tx(&db);
        let (tx, delete_result) = tx_slot.set(tx, f);

        delete_result?;

        let bytes_scanned = bsatn_rows.len();

        assert_eq!(0, tx.metrics.index_seeks);
        assert_eq!(3, tx.metrics.rows_scanned);
        assert_eq!(bytes_scanned, tx.metrics.bytes_scanned);
        assert_eq!(0, tx.metrics.bytes_written);
        assert_eq!(0, tx.metrics.bytes_sent_to_clients);
        Ok(())
    }
}
