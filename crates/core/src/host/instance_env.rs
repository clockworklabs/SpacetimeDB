use parking_lot::{Mutex, MutexGuard};
use smallvec::SmallVec;
use spacetimedb_table::table::UniqueConstraintViolation;
use std::ops::DerefMut;
use std::sync::Arc;

use super::scheduler::{ScheduleError, ScheduledReducerId, Scheduler};
use super::timestamp::Timestamp;
use crate::database_instance_context::DatabaseInstanceContext;
use crate::database_logger::{BacktraceProvider, LogLevel, Record};
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::error::{IndexError, NodesError};
use crate::execution_context::ExecutionContext;
use crate::util::ResultInspectExt;
use crate::vm::{DbProgram, TxMode};
use spacetimedb_lib::filter::CmpArgs;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::operator::OpQuery;
use spacetimedb_lib::{bsatn, ProductValue};
use spacetimedb_primitives::{ColId, ColListBuilder, TableId};
use spacetimedb_sats::buffer::BufWriter;
use spacetimedb_sats::db::def::{IndexDef, IndexType};
use spacetimedb_sats::relation::{FieldExpr, FieldName};
use spacetimedb_sats::{ProductType, Typespace};
use spacetimedb_vm::expr::{Code, ColumnOp};

#[derive(Clone)]
pub struct InstanceEnv {
    pub dbic: Arc<DatabaseInstanceContext>,
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

impl BufWriter for ChunkedWriter {
    fn put_slice(&mut self, slice: &[u8]) {
        self.scratch_space.extend_from_slice(slice);
    }
}

impl ChunkedWriter {
    /// Reserves `len` additional bytes in the scratch space,
    /// or does nothing if the capacity is already sufficient.
    fn reserve_in_scratch(&mut self, len: usize) {
        self.scratch_space.reserve(len);
    }

    /// Flushes the data collected in the scratch space if it's larger than our
    /// chunking threshold.
    pub fn flush(&mut self) {
        // For now, just send buffers over a certain fixed size.
        const ITER_CHUNK_SIZE: usize = 64 * 1024;

        if self.scratch_space.len() > ITER_CHUNK_SIZE {
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
}

// Generic 'instance environment' delegated to from various host types.
impl InstanceEnv {
    pub fn new(dbic: Arc<DatabaseInstanceContext>, scheduler: Scheduler) -> Self {
        Self {
            dbic,
            scheduler,
            tx: TxSlot::default(),
        }
    }

    #[tracing::instrument(skip_all, fields(reducer=reducer))]
    pub fn schedule(
        &self,
        reducer: String,
        args: Vec<u8>,
        time: Timestamp,
    ) -> Result<ScheduledReducerId, ScheduleError> {
        self.scheduler.schedule(reducer, args, time)
    }

    #[tracing::instrument(skip_all)]
    pub fn cancel_reducer(&self, id: ScheduledReducerId) {
        self.scheduler.cancel(id)
    }

    fn get_tx(&self) -> Result<impl DerefMut<Target = MutTxId> + '_, GetTxError> {
        self.tx.get()
    }

    #[tracing::instrument(skip_all)]
    pub fn console_log(&self, level: LogLevel, record: &Record, bt: &dyn BacktraceProvider) {
        self.dbic.logger.write(level, record, bt);
        log::trace!("MOD({}): {}", self.dbic.address.to_abbreviated_hex(), record.message);
    }

    pub fn insert(&self, ctx: &ExecutionContext, table_id: TableId, buffer: &[u8]) -> Result<ProductValue, NodesError> {
        let stdb = &*self.dbic.relational_db;
        let tx = &mut *self.get_tx()?;
        let ret = stdb
            .insert_bytes_as_row(tx, table_id, buffer)
            .inspect_err_(|e| match e {
                crate::error::DBError::Index(IndexError::UniqueConstraintViolation(UniqueConstraintViolation {
                    constraint_name: _,
                    table_name: _,
                    cols: _,
                    value: _,
                })) => {}
                _ => {
                    let res = stdb.table_name_from_id(ctx, tx, table_id);
                    if let Ok(Some(table_name)) = res {
                        log::debug!("insert(table: {table_name}, table_id: {table_id}): {e}")
                    } else {
                        log::debug!("insert(table_id: {table_id}): {e}")
                    }
                }
            })?;

        Ok(ret)
    }

    /// Deletes all rows in the table identified by `table_id`
    /// where the column identified by `cols` equates to `value`.
    ///
    /// Returns an error if no rows were deleted or if the column wasn't found.
    #[tracing::instrument(skip(self, ctx, value))]
    pub fn delete_by_col_eq(
        &self,
        ctx: &ExecutionContext,
        table_id: TableId,
        col_id: ColId,
        value: &[u8],
    ) -> Result<u32, NodesError> {
        let stdb = &*self.dbic.relational_db;
        let tx = &mut *self.get_tx()?;

        // Interpret the `value` using the schema of the column.
        let eq_value = stdb.decode_column(tx, table_id, col_id, value)?;

        // Find all rows in the table where the column data equates to `value`.
        let rows_to_delete = stdb
            .iter_by_col_eq_mut(ctx, tx, table_id, col_id, eq_value)?
            .map(|row_ref| row_ref.pointer())
            // `delete_by_field` only cares about 1 element,
            // so optimize for that.
            .collect::<SmallVec<[_; 1]>>();

        // Delete them and count how many we deleted.
        Ok(stdb.delete(tx, table_id, rows_to_delete))
    }

    /// Deletes all rows in the table identified by `table_id`
    /// where the rows match one in `relation`
    /// which is a bsatn encoding of `Vec<ProductValue>`.
    ///
    /// Returns an error if no rows were deleted.
    #[tracing::instrument(skip(self, relation))]
    pub fn delete_by_rel(&self, table_id: TableId, relation: &[u8]) -> Result<u32, NodesError> {
        let stdb = &*self.dbic.relational_db;
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
    /// Errors with `TableNotFound` if the table does not exist.
    #[tracing::instrument(skip_all)]
    pub fn get_table_id(&self, table_name: &str) -> Result<TableId, NodesError> {
        let stdb = &*self.dbic.relational_db;
        let tx = &mut *self.get_tx()?;

        // Query the table id from the name.
        let table_id = stdb
            .table_id_from_name_mut(tx, table_name)?
            .ok_or(NodesError::TableNotFound)?;

        Ok(table_id)
    }

    /// Creates an index of type `index_type` and name `index_name`,
    /// on a product of the given columns in `col_ids`,
    /// in the table identified by `table_id`.
    ///
    /// Currently only single-column-indices are supported.
    /// That is, `col_ids.len() == 1`, or the call will panic.
    ///
    /// Another limitation is on the `index_type`.
    /// Only `btree` indices are supported as of now, i.e., `index_type == 0`.
    /// When `index_type == 1` is passed, the call will happen
    /// and on `index_type > 1`, an error is returned.
    #[tracing::instrument(skip_all)]
    pub fn create_index(
        &self,
        index_name: String,
        table_id: TableId,
        index_type: u8,
        col_ids: Vec<u8>,
    ) -> Result<(), NodesError> {
        let stdb = &*self.dbic.relational_db;
        let tx = &mut *self.get_tx()?;

        // TODO(george) This check should probably move towards src/db/index, but right
        // now the API is pretty hardwired towards btrees.
        let index_type = IndexType::try_from(index_type).map_err(|_| NodesError::BadIndexType(index_type))?;
        match index_type {
            IndexType::BTree => {}
            IndexType::Hash => {
                todo!("Hash indexes not yet supported")
            }
        };

        let columns = col_ids
            .into_iter()
            .map(Into::into)
            .collect::<ColListBuilder>()
            .build()
            .expect("Attempt to create an index with zero columns");

        let is_unique = stdb.column_constraints(tx, table_id, &columns)?.has_unique();

        let index = IndexDef {
            columns,
            index_name,
            is_unique,
            index_type,
        };

        stdb.create_index(tx, table_id, index)?;

        Ok(())
    }

    /// Finds all rows in the table identified by `table_id`
    /// where the column identified by `cols` matches to `value`.
    ///
    /// These rows are returned concatenated with each row bsatn encoded.
    ///
    /// Matching is defined by decoding of `value` to an `AlgebraicValue`
    /// according to the column's schema and then `Ord for AlgebraicValue`.
    #[tracing::instrument(skip_all)]
    pub fn iter_by_col_eq(
        &self,
        ctx: &ExecutionContext,
        table_id: TableId,
        col_id: ColId,
        value: &[u8],
    ) -> Result<Vec<u8>, NodesError> {
        let stdb = &*self.dbic.relational_db;
        let tx = &mut *self.get_tx()?;

        // Interpret the `value` using the schema of the column.
        let value = stdb.decode_column(tx, table_id, col_id, value)?;

        // Find all rows in the table where the column data matches `value`.
        // Concatenate and return these rows using bsatn encoding.
        let results = stdb.iter_by_col_eq_mut(ctx, tx, table_id, col_id, value)?;
        let mut bytes = Vec::new();
        for result in results {
            // Pre-allocate the capacity needed to write `result`.
            bytes.reserve(bsatn::to_len(&result).unwrap());
            // Write the ref directly to the BSATN `bytes` buffer.
            bsatn::to_writer(&mut bytes, &result).unwrap();
        }
        Ok(bytes)
    }

    #[tracing::instrument(skip_all)]
    pub fn iter_chunks(&self, ctx: &ExecutionContext, table_id: TableId) -> Result<Vec<Box<[u8]>>, NodesError> {
        let mut chunked_writer = ChunkedWriter::default();

        let stdb = &*self.dbic.relational_db;
        let tx = &mut *self.tx.get()?;

        for row in stdb.iter_mut(ctx, tx, table_id)? {
            // Pre-allocate the capacity needed to write `row`.
            chunked_writer.reserve_in_scratch(bsatn::to_len(&row).unwrap());
            // Write the ref directly to the BSATN `chunked_writer` buffer.
            bsatn::to_writer(&mut chunked_writer, &row).unwrap();
            // Flush at row boundaries.
            chunked_writer.flush();
        }

        Ok(chunked_writer.into_chunks())
    }

    #[tracing::instrument(skip_all)]
    pub fn iter_filtered_chunks(
        &self,
        ctx: &ExecutionContext,
        table_id: TableId,
        filter: &[u8],
    ) -> Result<Vec<Box<[u8]>>, NodesError> {
        use spacetimedb_lib::filter;

        fn filter_to_column_op(table_name: &str, filter: filter::Expr) -> ColumnOp {
            match filter {
                filter::Expr::Cmp(filter::Cmp {
                    op,
                    args: CmpArgs { lhs_field, rhs },
                }) => ColumnOp::Cmp {
                    op: OpQuery::Cmp(op),
                    lhs: Box::new(ColumnOp::Field(FieldExpr::Name(FieldName::positional(
                        table_name,
                        lhs_field as usize,
                    )))),
                    rhs: Box::new(ColumnOp::Field(match rhs {
                        filter::Rhs::Field(rhs_field) => {
                            FieldExpr::Name(FieldName::positional(table_name, rhs_field as usize))
                        }
                        filter::Rhs::Value(rhs_value) => FieldExpr::Value(rhs_value),
                    })),
                },
                filter::Expr::Logic(filter::Logic { lhs, op, rhs }) => ColumnOp::Cmp {
                    op: OpQuery::Logic(op),
                    lhs: Box::new(filter_to_column_op(table_name, *lhs)),
                    rhs: Box::new(filter_to_column_op(table_name, *rhs)),
                },
                filter::Expr::Unary(_) => todo!("unary operations are not yet supported"),
            }
        }

        let stdb = &self.dbic.relational_db;
        let tx = &mut *self.tx.get()?;

        let schema = stdb.schema_for_table_mut(tx, table_id)?;
        let row_type = ProductType::from(&*schema);

        let filter = filter::Expr::from_bytes(
            // TODO: looks like module typespace is currently not hooked up to instances;
            // use empty typespace for now which should be enough for primitives
            // but figure this out later
            &Typespace::default(),
            &row_type.elements,
            filter,
        )
        .map_err(NodesError::DecodeFilter)?;
        let q = spacetimedb_vm::dsl::query(&*schema).with_select(filter_to_column_op(&schema.table_name, filter));
        //TODO: How pass the `caller` here?
        let mut tx: TxMode = tx.into();
        let p = &mut DbProgram::new(ctx, stdb, &mut tx, AuthCtx::for_current(self.dbic.identity));
        let results = match spacetimedb_vm::eval::run_ast(p, q.into()) {
            Code::Table(table) => table,
            _ => unreachable!("query should always return a table"),
        };

        let mut chunked_writer = ChunkedWriter::default();

        // write all rows and flush at row boundaries
        for row in results.data {
            row.data.encode(&mut chunked_writer);
            chunked_writer.flush();
        }

        Ok(chunked_writer.into_chunks())
    }
}

impl TxSlot {
    pub fn set<T>(&self, tx: MutTxId, f: impl FnOnce() -> T) -> (MutTxId, T) {
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
