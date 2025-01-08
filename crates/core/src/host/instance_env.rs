use super::scheduler::{get_schedule_from_row, ScheduleError, Scheduler};
use crate::database_logger::{BacktraceProvider, LogLevel, Record};
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::error::{IndexError, NodesError};
use crate::replica_context::ReplicaContext;
use core::mem;
use parking_lot::{Mutex, MutexGuard};
use smallvec::SmallVec;
use spacetimedb_primitives::{ColId, ColList, IndexId, TableId};
use spacetimedb_sats::{
    bsatn::{self, ToBsatn},
    buffer::{CountWriter, TeeWriter},
    AlgebraicValue, ProductValue,
};
use spacetimedb_table::table::{RowRef, UniqueConstraintViolation};
use std::ops::DerefMut;
use std::sync::Arc;

#[derive(Clone)]
pub struct InstanceEnv {
    pub replica_ctx: Arc<ReplicaContext>,
    pub scheduler: Scheduler,
    pub tx: TxSlot,
}

#[derive(Clone, Default)]
pub struct TxSlot {
    inner: Arc<Mutex<Option<MutTxId>>>,
}

/// A pool of available unused chunks.
///
/// The chunk places currently no limits on its size.
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

    /// Return a chunk back to the pool.
    pub fn put(&mut self, mut chunk: Vec<u8>) {
        chunk.clear();
        self.free_chunks.push(chunk);
    }
}

#[derive(Default)]
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

    /// Finalises the writer and returns all the chunks.
    fn into_chunks(mut self) -> Vec<Vec<u8>> {
        if !self.curr.is_empty() {
            self.chunks.push(self.curr);
        }
        self.chunks
    }

    pub fn collect_iter(pool: &mut ChunkPool, iter: impl Iterator<Item = impl ToBsatn>) -> Vec<Vec<u8>> {
        let mut chunked_writer = Self::default();
        // Consume the iterator, serializing each `item`,
        // while allowing a chunk to be created at boundaries.
        for item in iter {
            // Write the item directly to the BSATN `chunked_writer` buffer.
            item.to_bsatn_extend(&mut chunked_writer.curr).unwrap();
            // Flush at item boundaries.
            chunked_writer.flush(pool);
        }

        chunked_writer.into_chunks()
    }
}

// Generic 'instance environment' delegated to from various host types.
impl InstanceEnv {
    pub fn new(replica_ctx: Arc<ReplicaContext>, scheduler: Scheduler) -> Self {
        Self {
            replica_ctx,
            scheduler,
            tx: TxSlot::default(),
        }
    }

    fn get_tx(&self) -> Result<impl DerefMut<Target = MutTxId> + '_, GetTxError> {
        self.tx.get()
    }

    #[tracing::instrument(skip_all)]
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

        let (row_len, row_ptr) = stdb
            .insert(tx, table_id, buffer)
            .map(|(gen_cols, row_ref)| {
                let row_len = Self::project_cols_bsatn(buffer, gen_cols, row_ref);
                (row_len, row_ref.pointer())
            })
            .inspect_err(|e| match e {
                crate::error::DBError::Index(IndexError::UniqueConstraintViolation(UniqueConstraintViolation {
                    constraint_name: _,
                    table_name: _,
                    cols: _,
                    value: _,
                })) => {}
                _ => {
                    let res = stdb.table_name_from_id_mut(tx, table_id);
                    if let Ok(Some(table_name)) = res {
                        log::debug!("insert(table: {table_name}, table_id: {table_id}): {e}")
                    } else {
                        log::debug!("insert(table_id: {table_id}): {e}")
                    }
                }
            })?;

        if let Some((id_column, at_column)) = stdb.table_scheduled_id_and_at(tx, table_id)? {
            let row_ref = tx.get(table_id, row_ptr)?.unwrap();
            let (schedule_id, schedule_at) = get_schedule_from_row(&row_ref, id_column, at_column)
                // NOTE(centril): Should never happen,
                // as we successfully inserted and thus `ret` is verified against the table schema.
                .map_err(|e| NodesError::ScheduleError(ScheduleError::DecodingError(e)))?;
            self.scheduler
                .schedule(table_id, schedule_id, schedule_at, id_column, at_column)
                .map_err(NodesError::ScheduleError)?;
        }

        Ok(row_len)
    }

    #[tracing::instrument(skip_all)]
    pub fn datastore_delete_by_btree_scan_bsatn(
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
        let (table_id, iter) = stdb.btree_scan(tx, index_id, prefix, prefix_elems, rstart, rend)?;
        // Re. `SmallVec`, `delete_by_field` only cares about 1 element, so optimize for that.
        let rows_to_delete = iter.map(|row_ref| row_ref.pointer()).collect::<SmallVec<[_; 1]>>();

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
    #[tracing::instrument(skip(self, relation))]
    pub fn datastore_delete_all_by_eq_bsatn(&self, table_id: TableId, relation: &[u8]) -> Result<u32, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.get_tx()?;

        // Find the row schema using it to decode a vector of product values.
        let row_ty = stdb.row_schema_for_table(tx, table_id)?;
        // `TableType::delete` cares about a single element
        // so in that case we can avoid the allocation by using `smallvec`.
        let relation = ProductValue::decode_smallvec(&row_ty, &mut &*relation).map_err(NodesError::DecodeRow)?;

        // Delete them and return how many we deleted.
        Ok(stdb.delete_by_rel(tx, table_id, relation))
    }

    /// Returns the `table_id` associated with the given `table_name`.
    ///
    /// Errors with `GetTxError` if not in a transaction
    /// and `TableNotFound` if the table does not exist.
    #[tracing::instrument(skip_all)]
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
    #[tracing::instrument(skip_all)]
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
    #[tracing::instrument(skip_all)]
    pub fn datastore_table_row_count(&self, table_id: TableId) -> Result<u64, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.get_tx()?;

        // Query the row count for id.
        stdb.table_row_count_mut(tx, table_id).ok_or(NodesError::TableNotFound)
    }

    #[tracing::instrument(skip_all)]
    pub fn datastore_table_scan_bsatn_chunks(
        &self,
        pool: &mut ChunkPool,
        table_id: TableId,
    ) -> Result<Vec<Vec<u8>>, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.tx.get()?;

        let chunks = ChunkedWriter::collect_iter(pool, stdb.iter_mut(tx, table_id)?);
        Ok(chunks)
    }

    #[tracing::instrument(skip_all)]
    pub fn datastore_btree_scan_bsatn_chunks(
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

        let (_, iter) = stdb.btree_scan(tx, index_id, prefix, prefix_elems, rstart, rend)?;
        let chunks = ChunkedWriter::collect_iter(pool, iter);
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
