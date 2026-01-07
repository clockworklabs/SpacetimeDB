use super::scheduler::{get_schedule_from_row, ScheduleError, Scheduler};
use crate::database_logger::{BacktraceFrame, BacktraceProvider, LogLevel, ModuleBacktrace, Record};
use crate::db::relational_db::{MutTx, RelationalDB};
use crate::error::{DBError, DatastoreError, IndexError, NodesError};
use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent, ModuleFunctionCall};
use crate::host::wasm_common::TimingSpan;
use crate::replica_context::ReplicaContext;
use crate::subscription::module_subscription_actor::{commit_and_broadcast_event, ModuleSubscriptions};
use crate::subscription::module_subscription_manager::{from_tx_offset, TransactionOffset};
use crate::util::prometheus_handle::IntGaugeExt;
use chrono::{DateTime, Utc};
use core::mem;
use parking_lot::{Mutex, MutexGuard};
use smallvec::SmallVec;
use spacetimedb_client_api_messages::energy::EnergyQuanta;
use spacetimedb_datastore::db_metrics::DB_METRICS;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_datastore::locking_tx_datastore::{FuncCallType, MutTxId};
use spacetimedb_datastore::traits::IsolationLevel;
use spacetimedb_lib::{http as st_http, ConnectionId, Identity, Timestamp};
use spacetimedb_primitives::{ColId, ColList, IndexId, TableId};
use spacetimedb_sats::{
    bsatn::{self, ToBsatn},
    buffer::{CountWriter, TeeWriter},
    AlgebraicValue, ProductValue,
};
use spacetimedb_table::indexes::RowPointer;
use spacetimedb_table::table::RowRef;
use std::fmt::Display;
use std::future::Future;
use std::ops::DerefMut;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::vec::IntoIter;

pub struct InstanceEnv {
    pub replica_ctx: Arc<ReplicaContext>,
    pub scheduler: Scheduler,
    pub tx: TxSlot,
    /// The timestamp the current function began running.
    pub start_time: Timestamp,
    /// The instant the current function began running.
    pub start_instant: Instant,
    /// The type of the last, including current, function to be executed by this environment.
    pub func_type: FuncCallType,
    /// The name of the last, including current, function to be executed by this environment.
    pub func_name: String,
    /// Are we in an anonymous tx context?
    in_anon_tx: bool,
    /// A procedure's last known transaction offset.
    procedure_last_tx_offset: Option<TransactionOffset>,
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
    ) -> (Vec<Vec<u8>>, usize, usize) {
        // Track the number of rows and the number of bytes scanned by the iterator.
        let mut rows_scanned = 0;
        let mut bytes_scanned = 0;

        let mut chunked_writer = Self::new(pool);
        // Consume the iterator, serializing each `item`,
        // while allowing a chunk to be created at boundaries.
        for item in iter {
            // Write the item directly to the BSATN `chunked_writer` buffer.
            item.to_bsatn_extend(&mut chunked_writer.curr).unwrap();
            // Flush at item boundaries.
            chunked_writer.flush(pool);
            // Update rows scanned.
            rows_scanned += 1;
        }

        let chunks = chunked_writer.into_chunks();

        // Update (BSATN) bytes scanned
        bytes_scanned += chunks.iter().map(|chunk| chunk.len()).sum::<usize>();

        (chunks, rows_scanned, bytes_scanned)
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
            start_instant: Instant::now(),
            // arbitrary - change if we need to recognize that an `InstanceEnv` has never
            // run a function
            func_type: FuncCallType::Reducer,
            func_name: String::from("<initializing>"),
            in_anon_tx: false,
            procedure_last_tx_offset: None,
        }
    }

    /// Returns the database's identity.
    pub fn database_identity(&self) -> &Identity {
        &self.replica_ctx.database.database_identity
    }

    /// Signal to this `InstanceEnv` that a function call is beginning.
    pub fn start_funcall(&mut self, name: &str, ts: Timestamp, func_type: FuncCallType) {
        self.start_time = ts;
        self.start_instant = Instant::now();
        self.func_type = func_type;
        name.clone_into(&mut self.func_name);
    }

    fn get_tx(&self) -> Result<impl DerefMut<Target = MutTxId> + '_, GetTxError> {
        self.tx.get()
    }

    /// True if `self` is holding an open transaction, or false if it is not.
    pub fn in_tx(&self) -> bool {
        self.get_tx().is_ok()
    }

    pub(crate) fn take_tx(&self) -> Result<MutTxId, GetTxError> {
        self.tx.take()
    }

    pub(crate) fn relational_db(&self) -> &Arc<RelationalDB> {
        &self.replica_ctx.relational_db
    }

    pub(crate) fn get_jwt_payload(&self, connection_id: ConnectionId) -> Result<Option<String>, NodesError> {
        let tx = &mut *self.get_tx()?;
        Ok(tx.get_jwt_payload(connection_id).map_err(DBError::from)?)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn console_log(&self, level: LogLevel, record: &Record, bt: &dyn BacktraceProvider) {
        self.replica_ctx.logger.write(level, record, bt);
        log::trace!(
            "MOD({}): {}",
            self.replica_ctx.database_identity.to_abbreviated_hex(),
            record.message
        );
    }

    /// Logs a simple `message` at `level`.
    pub(crate) fn console_log_simple_message(&self, level: LogLevel, function: Option<&str>, message: &str) {
        /// A backtrace provider that provides nothing.
        struct Noop;
        impl BacktraceProvider for Noop {
            fn capture(&self) -> Box<dyn ModuleBacktrace> {
                Box::new(Noop)
            }
        }
        impl ModuleBacktrace for Noop {
            fn frames(&self) -> Vec<BacktraceFrame<'_>> {
                Vec::new()
            }
        }

        let record = Record {
            ts: Self::now_for_logging(),
            target: None,
            filename: None,
            line_number: None,
            function,
            message,
        };
        self.console_log(level, &record, &Noop);
    }

    /// End a console timer by logging the span at INFO level.
    pub(crate) fn console_timer_end(&self, span: &TimingSpan, function: Option<&str>) {
        let elapsed = span.start.elapsed();
        let message = format!("Timing span {:?}: {:?}", &span.name, elapsed);

        self.console_log_simple_message(LogLevel::Info, function, &message);
    }

    /// Returns the current time suitable for logging.
    pub fn now_for_logging() -> DateTime<Utc> {
        // TODO: figure out whether to use walltime now or logical reducer now (env.reducer_start).
        chrono::Utc::now()
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
        let stdb = self.relational_db();
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
                    DBError::Datastore(DatastoreError::Index(IndexError::UniqueConstraintViolation(_))) => {}
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

        let row_ref = tx.get(table_id, row_ptr).map_err(DBError::from)?.unwrap();
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
        let stdb = self.relational_db();
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
                    DBError::Datastore(DatastoreError::Index(IndexError::UniqueConstraintViolation(_))) => {}
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
    pub fn datastore_delete_by_index_scan_point_bsatn(
        &self,
        index_id: IndexId,
        point: &[u8],
    ) -> Result<u32, NodesError> {
        let stdb = self.relational_db();
        let tx = &mut *self.get_tx()?;

        // Find all rows in the table to delete.
        let (table_id, _, iter) = stdb.index_scan_point(tx, index_id, point)?;
        // Re. `SmallVec`, `delete_by_field` only cares about 1 element, so optimize for that.
        let rows_to_delete = iter.map(|row_ref| row_ref.pointer()).collect::<SmallVec<[_; 1]>>();

        Ok(Self::datastore_delete_by_index_scan(stdb, tx, table_id, rows_to_delete))
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
        let stdb = self.relational_db();
        let tx = &mut *self.get_tx()?;

        // Find all rows in the table to delete.
        let (table_id, _, _, iter) = stdb.index_scan_range(tx, index_id, prefix, prefix_elems, rstart, rend)?;
        // Re. `SmallVec`, `delete_by_field` only cares about 1 element, so optimize for that.
        let rows_to_delete = iter.map(|row_ref| row_ref.pointer()).collect::<SmallVec<[_; 1]>>();

        Ok(Self::datastore_delete_by_index_scan(stdb, tx, table_id, rows_to_delete))
    }

    /// Deletes `rows_to_delete` in `tx`
    /// and assumes `rows_to_delete` came from an index scan.
    fn datastore_delete_by_index_scan(
        stdb: &RelationalDB,
        tx: &mut MutTxId,
        table_id: TableId,
        rows_to_delete: SmallVec<[RowPointer; 1]>,
    ) -> u32 {
        // Note, we're deleting rows based on the result of an index scan.
        // Hence we must update our `index_seeks` and `rows_scanned` metrics.
        //
        // Note that we're not updating `bytes_scanned` at all,
        // because we never dereference any of the returned `RowPointer`s.
        tx.metrics.index_seeks += 1;
        tx.metrics.rows_scanned += rows_to_delete.len();

        // Delete them and count how many we deleted.
        stdb.delete(tx, table_id, rows_to_delete)
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
        let stdb = self.relational_db();
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
        let stdb = self.relational_db();
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
        let stdb = self.relational_db();
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
        let stdb = self.relational_db();
        let tx = &mut *self.get_tx()?;

        // Query the row count for id.
        stdb.table_row_count_mut(tx, table_id)
            .ok_or(NodesError::TableNotFound)
            .inspect(|_| {
                tx.record_table_scan(&self.func_type, table_id);
            })
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn datastore_table_scan_bsatn_chunks(
        &self,
        pool: &mut ChunkPool,
        table_id: TableId,
    ) -> Result<Vec<Vec<u8>>, NodesError> {
        let tx = &mut *self.get_tx()?;

        // Open the iterator.
        let iter = self.relational_db().iter_mut(tx, table_id)?;

        // Scan the index and serialize rows to BSATN.
        let (chunks, rows_scanned, bytes_scanned) = ChunkedWriter::collect_iter(pool, iter);

        // Record the number of rows and the number of bytes scanned by the iterator.
        tx.metrics.bytes_scanned += bytes_scanned;
        tx.metrics.rows_scanned += rows_scanned;

        tx.record_table_scan(&self.func_type, table_id);

        Ok(chunks)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn datastore_index_scan_point_bsatn_chunks(
        &self,
        pool: &mut ChunkPool,
        index_id: IndexId,
        point: &[u8],
    ) -> Result<Vec<Vec<u8>>, NodesError> {
        let tx = &mut *self.get_tx()?;

        // Open index iterator
        let (table_id, point, iter) = self.relational_db().index_scan_point(tx, index_id, point)?;

        // Scan the index and serialize rows to BSATN.
        let (chunks, rows_scanned, bytes_scanned) = ChunkedWriter::collect_iter(pool, iter);

        // Record the number of rows and the number of bytes scanned by the iterator.
        tx.metrics.index_seeks += 1;
        tx.metrics.bytes_scanned += bytes_scanned;
        tx.metrics.rows_scanned += rows_scanned;

        tx.record_index_scan_point(&self.func_type, table_id, index_id, point);

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
        let tx = &mut *self.get_tx()?;

        // Open index iterator
        let (table_id, lower, upper, iter) =
            self.relational_db()
                .index_scan_range(tx, index_id, prefix, prefix_elems, rstart, rend)?;

        // Scan the index and serialize rows to BSATN.
        let (chunks, rows_scanned, bytes_scanned) = ChunkedWriter::collect_iter(pool, iter);

        // Record the number of rows and the number of bytes scanned by the iterator.
        tx.metrics.index_seeks += 1;
        tx.metrics.bytes_scanned += bytes_scanned;
        tx.metrics.rows_scanned += rows_scanned;

        tx.record_index_scan_range(&self.func_type, table_id, index_id, lower, upper);

        Ok(chunks)
    }

    pub fn fill_buffer_from_iter(
        iter: &mut IntoIter<Vec<u8>>,
        mut buffer: &mut [u8],
        chunk_pool: &mut ChunkPool,
    ) -> usize {
        let mut written = 0;
        // Fill the buffer as much as possible.
        while let Some(chunk) = iter.as_slice().first() {
            let Some((buf_chunk, rest)) = buffer.split_at_mut_checked(chunk.len()) else {
                // Cannot fit chunk into the buffer,
                // either because we already filled it too much,
                // or because it is too small.
                break;
            };
            buf_chunk.copy_from_slice(chunk);
            written += chunk.len();
            buffer = rest;

            // Advance the iterator, as we used a chunk.
            // SAFETY: We peeked one `chunk`, so there must be one at least.
            let chunk = unsafe { iter.next().unwrap_unchecked() };
            chunk_pool.put(chunk);
        }

        written
    }

    // Async procedure syscalls return a `Result<impl Future>`, so that we can check `get_tx()`
    // *before* requiring an async runtime. Otherwise, the v8 module host would have to call
    // on `tokio::runtime::Handle::try_current()` before being able to run the `get_tx()` check.

    pub fn start_mutable_tx(&mut self) -> Result<(), NodesError> {
        if self.get_tx().is_ok() {
            return Err(NodesError::WouldBlockTransaction(
                super::AbiCall::ProcedureStartMutTransaction,
            ));
        }

        let stdb = self.replica_ctx.relational_db.clone();
        // TODO(procedure-tx): should we add a new workload, e.g., `AnonTx`?
        let tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        self.tx.set_raw(tx);
        self.in_anon_tx = true;

        Ok(())
    }

    /// Finishes an anonymous transaction,
    /// returning `Some(_)` if there was no ongoing one,
    /// in which case the caller should return early.
    fn finish_anon_tx(&mut self) -> Result<(), NodesError> {
        if self.in_anon_tx {
            self.in_anon_tx = false;
            Ok(())
        } else {
            // Not in an anon tx context.
            // This can happen if a reducer calls this ABI
            // and tries to commit its own transaction early.
            // We refuse to do this, as it would cause a later panic in the host.
            Err(NodesError::NotInAnonTransaction)
        }
    }

    // Async procedure syscalls return a `Result<impl Future>`, so that we can check `get_tx()`
    // *before* requiring an async runtime. Otherwise, the v8 module host would have to call
    // on `tokio::runtime::Handle::try_current()` before being able to run the `get_tx()` check.

    pub fn commit_mutable_tx(&mut self) -> Result<(), NodesError> {
        self.finish_anon_tx()?;

        let stdb = self.relational_db().clone();
        let tx = self.take_tx()?;
        let subs = self.replica_ctx.subscriptions.clone();

        let event = ModuleEvent {
            timestamp: Timestamp::now(),
            caller_identity: stdb.database_identity(),
            caller_connection_id: None,
            function_call: ModuleFunctionCall::default(),
            status: EventStatus::Committed(DatabaseUpdate::default()),
            request_id: None,
            timer: None,
            // The procedure will pick up the tab for the energy.
            energy_quanta_used: EnergyQuanta { quanta: 0 },
            host_execution_duration: Duration::from_millis(0),
        };
        // Commit the tx and broadcast it.
        let event = commit_and_broadcast_event(&subs, None, event, tx);
        self.procedure_last_tx_offset = Some(event.tx_offset);

        Ok(())
    }

    pub fn abort_mutable_tx(&mut self) -> Result<(), NodesError> {
        self.finish_anon_tx()?;
        let stdb = self.relational_db().clone();
        let tx = self.take_tx()?;

        // Roll back the tx.
        let offset = ModuleSubscriptions::rollback_mut_tx(&stdb, tx);
        self.procedure_last_tx_offset = Some(from_tx_offset(offset));
        Ok(())
    }

    /// In-case there is a anonymous tx at the end of a procedure,
    /// it must be terminated.
    ///
    /// This represents a misuse by the module author of the module ABI.
    pub fn terminate_dangling_anon_tx(&mut self) {
        // Try to abort the anon tx.
        match self.abort_mutable_tx() {
            // There was no dangling anon tx. Yay!
            Err(NodesError::NotInAnonTransaction) => {}
            // There was one, which has been aborted.
            // The module is using the ABI wrong! 😭
            Ok(()) => {
                let message = format!(
                    "aborting dangling anonymous transaction in procedure {}",
                    self.func_name
                );
                self.console_log_simple_message(LogLevel::Error, None, &message);
            }
            res => unreachable!("should've had a tx to close; {res:?}"),
        }
    }

    /// After a procedure has finished, take its known last tx offset, if any.
    pub fn take_procedure_tx_offset(&mut self) -> Option<TransactionOffset> {
        self.procedure_last_tx_offset.take()
    }

    /// Perform an HTTP request.
    /// Exposed to modules via the `ProcedureContext`.
    ///
    /// It's very important that the error returned from this function
    /// not contain any potentially sensitive data from `request`,
    /// such as the query parameters or header values.
    /// This way, it's safe to log the errors (either for us to do so, or for module code to do so),
    /// and less dangerous to send them to the calling client of a procedure.
    pub fn http_request(
        &mut self,
        request: st_http::Request,
        body: bytes::Bytes,
    ) -> Result<impl Future<Output = Result<(st_http::Response, bytes::Bytes), NodesError>>, NodesError> {
        if self.in_tx() {
            // If we're holding a transaction open, refuse to perform this blocking operation.
            return Err(NodesError::WouldBlockTransaction(super::AbiCall::ProcedureHttpRequest));
        }

        // Record in metrics that we're starting an HTTP request.
        DB_METRICS
            .procedure_num_http_requests
            .with_label_values(self.database_identity())
            .inc();
        DB_METRICS
            .procedure_http_request_size_bytes
            .with_label_values(self.database_identity())
            .inc_by((request.size_in_bytes() + body.len()) as _);
        // Make a guard for the `in_progress` metric that will be decremented on exit.
        let _in_progress_metric = DB_METRICS
            .procedure_num_in_progress_http_requests
            .with_label_values(self.database_identity())
            .inc_scope();

        /// Strip the query part out of the URL in `err`, as query parameters may be sensitive
        /// and we'd like it to be safe to directly log errors from this method.
        fn strip_query_params_from_reqwest_error(mut err: reqwest::Error) -> reqwest::Error {
            if let Some(url) = err.url_mut() {
                // `set_query` of `None` clears the query part.
                url.set_query(None);
            }
            err
        }

        fn http_error<E: ToString>(err: E) -> NodesError {
            NodesError::HttpError(err.to_string())
        }

        // Then convert the request into an `http::Request`, a semi-standard "lingua franca" type in the Rust ecosystem,
        // and map its body into a type `reqwest` will like.
        //
        // See comments on and in `convert_http_request` for justification that there's no sensitive info in this error.
        let (request, timeout) = convert_http_request(request).map_err(http_error)?;

        let request = http::Request::from_parts(request, body);

        let mut reqwest: reqwest::Request = request
            .try_into()
            // `reqwest::Error` may contain sensitive info, namely the full URL with query params.
            // Strip those out before returning the error.
            .map_err(strip_query_params_from_reqwest_error)
            .map_err(http_error)?;

        // If the user requested a timeout using our extension, slot it in to reqwest's timeout.
        // Clamp to the range `0..HTTP_DEFAULT_TIMEOUT`.
        let timeout = timeout.unwrap_or(HTTP_DEFAULT_TIMEOUT).min(HTTP_DEFAULT_TIMEOUT);

        // reqwest's timeout covers from the start of the request to the end of reading the body,
        // so there's no need to do our own timeout operation.
        *reqwest.timeout_mut() = Some(timeout);

        let reqwest = reqwest;

        // TODO(procedure-metrics): record size in bytes of response, time spent awaiting response.

        // Actually execute the HTTP request!
        // TODO(perf): Stash a long-lived `Client` in the env somewhere, rather than building a new one for each call.
        let execute_fut = reqwest::Client::new().execute(reqwest);

        let response_fut = async {
            // `reqwest::Error` may contain sensitive info, namely the full URL with query params.
            // We'll strip those with `strip_query_params_from_eqwest_error`
            // after `await`ing `response_fut` below.
            let response = execute_fut.await?;

            // Download the response body, which in all likelihood will be a stream,
            // as reqwest seems to prefer that.
            let (response, body) = http::Response::from(response).into_parts();

            // This error may also contain the full URL with query params.
            // Again, we'll strip them after `await`ing `response_fut` below.
            let body = http_body_util::BodyExt::collect(body).await?.to_bytes();

            Ok((response, body))
        };

        let database_identity = *self.database_identity();

        Ok(async move {
            let (response, body) = response_fut
                .await
                .inspect_err(|err: &reqwest::Error| {
                    // Report the request's failure in our metrics as either a timeout or a misc. failure, as appropriate.
                    if err.is_timeout() {
                        DB_METRICS
                            .procedure_num_timeout_http_requests
                            .with_label_values(&database_identity)
                            .inc();
                    } else {
                        DB_METRICS
                            .procedure_num_failed_http_requests
                            .with_label_values(&database_identity)
                            .inc();
                    }
                })
                // `response_fut` returns a `reqwest::Error`, which may contain the full URL including query params.
                // Strip them out to clean the error of potentially sensitive info.
                .map_err(strip_query_params_from_reqwest_error)
                .map_err(http_error)?;

            // Transform the `http::Response` into our `spacetimedb_lib::http::Response` type,
            // which has a stable BSATN encoding to pass across the WASM boundary.
            let response = convert_http_response(response);

            // Record the response size in bytes.
            DB_METRICS
                .procedure_http_response_size_bytes
                .with_label_values(&database_identity)
                .inc_by((response.size_in_bytes() + body.len()) as _);

            Ok((response, body))
        })
    }
}

/// Default / maximum timeout for HTTP requests performed by [`InstanceEnv::http_request`].
///
/// If the user requests a timeout longer than this, we will clamp to this value.
///
/// Value chosen arbitrarily by pgoldman 2025-11-18, based on little more than a vague guess.
const HTTP_DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);

/// Unpack `request` and convert it into an [`http::request::Parts`],
/// and a [`Duration`] from its `timeout` if supplied.
///
/// It's very important that the error return from this function
/// not contain any potentially sensitive data from `request`,
/// such as the query parameters or header values.
/// See comment on [`InstanceEnv::http_request`].
fn convert_http_request(request: st_http::Request) -> http::Result<(http::request::Parts, Option<Duration>)> {
    let st_http::Request {
        method,
        headers,
        timeout,
        uri,
        version,
    } = request;

    let (mut request, ()) = http::Request::new(()).into_parts();
    request.method = match method {
        st_http::Method::Get => http::Method::GET,
        st_http::Method::Head => http::Method::HEAD,
        st_http::Method::Post => http::Method::POST,
        st_http::Method::Put => http::Method::PUT,
        st_http::Method::Delete => http::Method::DELETE,
        st_http::Method::Connect => http::Method::CONNECT,
        st_http::Method::Options => http::Method::OPTIONS,
        st_http::Method::Trace => http::Method::TRACE,
        st_http::Method::Patch => http::Method::PATCH,
        st_http::Method::Extension(method) => http::Method::from_bytes(method.as_bytes()).expect("Invalid HTTP method"),
    };
    // The error type here, `http::uri::InvalidUri`, doesn't contain the URI itself,
    // so it's safe to return and to log.
    // See https://docs.rs/http/1.3.1/src/http/uri/mod.rs.html#120-141 .
    request.uri = uri.try_into()?;
    request.version = match version {
        st_http::Version::Http09 => http::Version::HTTP_09,
        st_http::Version::Http10 => http::Version::HTTP_10,
        st_http::Version::Http11 => http::Version::HTTP_11,
        st_http::Version::Http2 => http::Version::HTTP_2,
        st_http::Version::Http3 => http::Version::HTTP_3,
    };
    request.headers = headers
        .into_iter()
        .map(|(k, v)| {
            Ok((
                // The error type here, `http::header::InvalidHeaderName`, doesn't contain the header name itself,
                // so it's safe to return and to log.
                // See https://docs.rs/http/1.3.1/src/http/header/name.rs.html#60-63 .
                k.into_string().try_into()?,
                // The error type here, `http::header::InvalidHeaderValue`, doesn't contain the header value itself,
                // so it's safe to return and to log.
                // See https://docs.rs/http/1.3.1/src/http/header/value.rs.html#27-31 .
                v.into_vec().try_into()?,
            ))
        })
        // Collecting into a `HeaderMap` doesn't add any new possible errors,
        // the `?` here is just to propogate the errors from converting the individual header names and values.
        // We know those are free from sensitive info, so this result is clean.
        .collect::<http::Result<_>>()?;

    let timeout = timeout.map(|d| d.to_duration_saturating());

    Ok((request, timeout))
}

fn convert_http_response(response: http::response::Parts) -> st_http::Response {
    let http::response::Parts {
        extensions,
        headers,
        status,
        version,
        ..
    } = response;

    // there's a good chance that reqwest inserted some extensions into this request,
    // but we can't control that and don't care much about it.
    let _ = extensions;

    st_http::Response {
        headers: headers
            .into_iter()
            .map(|(k, v)| (k.map(|k| k.as_str().into()), v.as_bytes().into()))
            .collect(),
        version: match version {
            http::Version::HTTP_09 => st_http::Version::Http09,
            http::Version::HTTP_10 => st_http::Version::Http10,
            http::Version::HTTP_11 => st_http::Version::Http11,
            http::Version::HTTP_2 => st_http::Version::Http2,
            http::Version::HTTP_3 => st_http::Version::Http3,
            _ => unreachable!("Unknown HTTP version: {version:?}"),
        },
        code: status.as_u16(),
    }
}

impl TxSlot {
    /// Sets the slot to `tx`, ensuring that there was no tx before.
    pub fn set_raw(&mut self, tx: MutTxId) {
        let prev = self.inner.lock().replace(tx);
        assert!(prev.is_none(), "reentrant TxSlot::set");
    }

    /// Sets the slot to `tx` runs `work`, and returns back `tx`.
    pub fn set<T>(&mut self, tx: MutTxId, work: impl FnOnce() -> T) -> (MutTxId, T) {
        self.set_raw(tx);

        let remove_tx = || self.take().expect("tx was removed during transaction");

        let res = {
            scopeguard::defer_on_unwind! { remove_tx(); }
            work()
        };

        let tx = remove_tx();
        (tx, res)
    }

    /// Returns the tx in the slot.
    pub fn get(&self) -> Result<impl DerefMut<Target = MutTxId> + '_, GetTxError> {
        MutexGuard::try_map(self.inner.lock(), |map| map.as_mut()).map_err(|_| GetTxError)
    }

    /// Steals the tx from the slot.
    pub fn take(&self) -> Result<MutTxId, GetTxError> {
        self.inner.lock().take().ok_or(GetTxError)
    }
}

#[derive(Debug)]
pub struct GetTxError;
impl From<GetTxError> for NodesError {
    fn from(_: GetTxError) -> Self {
        NodesError::NotInTransaction
    }
}

impl Display for GetTxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "not in a transaction")
    }
}
impl std::error::Error for GetTxError {}

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
        let path = ModuleLogsDir::from_path_unchecked(temp.keep());
        let path = path.today();
        Ok(DatabaseLogger::open(path))
    }

    /// An `InstanceEnv` requires a `ReplicaContext`.
    /// For our purposes this is just a wrapper for `RelationalDB`.
    fn replica_ctx(relational_db: Arc<RelationalDB>) -> Result<(ReplicaContext, tokio::runtime::Runtime)> {
        let (subs, runtime) = ModuleSubscriptions::for_test_new_runtime(relational_db.clone());
        Ok((
            ReplicaContext {
                database: Database {
                    id: 0,
                    database_identity: Identity::ZERO,
                    owner_identity: Identity::ZERO,
                    host_type: HostType::Wasm,
                    initial_program: Hash::ZERO,
                },
                replica_id: 0,
                logger: Arc::new(temp_logger()?),
                subscriptions: subs,
                relational_db,
            },
            runtime,
        ))
    }

    /// An `InstanceEnv` used for testing the database syscalls.
    fn instance_env(db: Arc<RelationalDB>) -> Result<(InstanceEnv, tokio::runtime::Runtime)> {
        let (scheduler, _) = Scheduler::open(db.clone());
        let (replica_context, runtime) = replica_ctx(db)?;
        Ok((InstanceEnv::new(Arc::new(replica_context), scheduler), runtime))
    }

    /// An in-memory `RelationalDB` for testing.
    /// It does not persist data to disk.
    fn relational_db() -> Result<Arc<RelationalDB>> {
        let TestDB { db, .. } = TestDB::in_memory()?;
        Ok(db)
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
        let (env, _runtime) = instance_env(db.clone())?;

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
        let (env, _runtime) = instance_env(db.clone())?;

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
        let (env, _runtime) = instance_env(db.clone())?;

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
        let (env, _runtime) = instance_env(db.clone())?;

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
        let (env, _runtime) = instance_env(db.clone())?;

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
        let (env, _runtime) = instance_env(db.clone())?;

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
