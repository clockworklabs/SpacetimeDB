use super::scheduler::{get_schedule_from_row, ScheduleError, Scheduler};
use crate::database_logger::{BacktraceProvider, LogLevel, Record};
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::error::{IndexError, NodesError};
use crate::replica_context::ReplicaContext;
use parking_lot::{Mutex, MutexGuard};
use smallvec::SmallVec;
use spacetimedb_primitives::{ColId, IndexId, TableId};
use spacetimedb_sats::{bsatn::ToBsatn, AlgebraicValue, ProductValue};
use spacetimedb_table::table::UniqueConstraintViolation;
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

#[derive(Default)]
struct ChunkedWriter {
    chunks: Vec<Box<[u8]>>,
    scratch_space: Vec<u8>,
}

impl ChunkedWriter {
    /// Flushes the data collected in the scratch space if it's larger than our
    /// chunking threshold.
    pub fn flush(&mut self) {
        if self.scratch_space.len() > spacetimedb_primitives::ROW_ITER_CHUNK_SIZE {
            // We intentionally clone here so that our scratch space is not
            // recreated with zero capacity (via `Vec::new`), but instead can
            // be `.clear()`ed in-place and reused.
            //
            // This way the buffers in `chunks` are always fitted fixed-size to
            // the actual data they contain, while the scratch space is ever-
            // growing and has higher chance of fitting each next row without
            // reallocation.
            self.chunks.push(self.scratch_space.as_slice().into());
            self.scratch_space.clear();
        }
    }

    /// Finalises the writer and returns all the chunks.
    pub fn into_chunks(mut self) -> Vec<Box<[u8]>> {
        if !self.scratch_space.is_empty() {
            // Avoid extra clone by just shrinking and pushing the scratch space
            // in-place.
            self.chunks.push(self.scratch_space.into());
        }
        self.chunks
    }

    pub fn collect_iter(iter: impl Iterator<Item = impl ToBsatn>) -> Vec<Box<[u8]>> {
        let mut chunked_writer = Self::default();
        for item in iter {
            // Write the item directly to the BSATN `chunked_writer` buffer.
            item.to_bsatn_extend(&mut chunked_writer.scratch_space).unwrap();
            // Flush at item boundaries.
            chunked_writer.flush();
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

    pub fn insert(&self, table_id: TableId, buffer: &[u8]) -> Result<AlgebraicValue, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.get_tx()?;

        let (gen_cols, row_ptr) = stdb
            .insert_bytes_as_row(tx, table_id, buffer)
            .map(|(gc, rr)| (gc, rr.pointer()))
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

        Ok(gen_cols)
    }

    /// Deletes all rows in the table identified by `table_id`
    /// where the column identified by `cols` equates to `value`.
    ///
    /// Returns an error if no rows were deleted or if the column wasn't found.
    pub fn delete_by_col_eq(&self, table_id: TableId, col_id: ColId, value: &[u8]) -> Result<u32, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.get_tx()?;

        // Interpret the `value` using the schema of the column.
        let eq_value = &stdb.decode_column(tx, table_id, col_id, value)?;

        // Find all rows in the table where the column data equates to `value`.
        let rows_to_delete = stdb
            .iter_by_col_eq_mut(tx, table_id, col_id, eq_value)?
            .map(|row_ref| row_ref.pointer())
            // `delete_by_field` only cares about 1 element,
            // so optimize for that.
            .collect::<SmallVec<[_; 1]>>();

        // Delete them and count how many we deleted.
        Ok(stdb.delete(tx, table_id, rows_to_delete))
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

    /// Finds all rows in the table identified by `table_id`
    /// where the column identified by `cols` matches to `value`.
    ///
    /// These rows are returned concatenated with each row bsatn encoded.
    ///
    /// Matching is defined by decoding of `value` to an `AlgebraicValue`
    /// according to the column's schema and then `Ord for AlgebraicValue`.
    pub fn iter_by_col_eq_chunks(
        &self,
        table_id: TableId,
        col_id: ColId,
        value: &[u8],
    ) -> Result<Vec<Box<[u8]>>, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.get_tx()?;

        // Interpret the `value` using the schema of the column.
        let value = &stdb.decode_column(tx, table_id, col_id, value)?;

        // Find all rows in the table where the column data matches `value`.
        let chunks = ChunkedWriter::collect_iter(stdb.iter_by_col_eq_mut(tx, table_id, col_id, value)?);
        Ok(chunks)
    }

    #[tracing::instrument(skip_all)]
    pub fn datastore_table_scan_bsatn_chunks(&self, table_id: TableId) -> Result<Vec<Box<[u8]>>, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.tx.get()?;

        let chunks = ChunkedWriter::collect_iter(stdb.iter_mut(tx, table_id)?);
        Ok(chunks)
    }

    #[tracing::instrument(skip_all)]
    pub fn datastore_btree_scan_bsatn_chunks(
        &self,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<Vec<Box<[u8]>>, NodesError> {
        let stdb = &*self.replica_ctx.relational_db;
        let tx = &mut *self.tx.get()?;

        let (_, iter) = stdb.btree_scan(tx, index_id, prefix, prefix_elems, rstart, rend)?;
        let chunks = ChunkedWriter::collect_iter(iter);
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
