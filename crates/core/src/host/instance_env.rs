use nonempty::NonEmpty;
use parking_lot::{Mutex, MutexGuard};
use spacetimedb_lib::{bsatn, ProductValue};
use std::ops::DerefMut;
use std::sync::Arc;

use crate::database_instance_context::DatabaseInstanceContext;
use crate::database_logger::{BacktraceProvider, LogLevel, Record};
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::db::datastore::traits::{DataRow, IndexDef};
use crate::error::{IndexError, NodesError};
use crate::util::ResultInspectExt;

use super::scheduler::{ScheduleError, ScheduledReducerId, Scheduler};
use super::timestamp::Timestamp;
use crate::vm::DbProgram;
use spacetimedb_lib::filter::CmpArgs;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::operator::OpQuery;
use spacetimedb_lib::relation::{FieldExpr, FieldName};
use spacetimedb_sats::{ProductType, SatsString, Typespace};
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

// Generic 'instance environment' delegated to from various host types.
impl InstanceEnv {
    pub fn new(dbic: Arc<DatabaseInstanceContext>, scheduler: Scheduler) -> Self {
        Self {
            dbic,
            scheduler,
            tx: TxSlot::default(),
        }
    }

    #[tracing::instrument(skip_all)]
    pub fn schedule(
        &self,
        reducer: SatsString,
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
        self.dbic.logger.lock().unwrap().write(level, record, bt);
        log::trace!("MOD({}): {}", self.dbic.address.to_abbreviated_hex(), record.message);
    }

    pub fn insert(&self, table_id: u32, buffer: &[u8]) -> Result<ProductValue, NodesError> {
        let stdb = &*self.dbic.relational_db;
        let tx = &mut *self.get_tx()?;

        let ret = stdb
            .insert_bytes_as_row(tx, table_id, buffer)
            .inspect_err_(|e| match e {
                crate::error::DBError::Index(IndexError::UniqueConstraintViolation {
                    constraint_name: _,
                    table_name: _,
                    col_names: _,
                    value: _,
                }) => {}
                _ => {
                    let res = stdb.table_name_from_id(tx, table_id);
                    if let Ok(Some(table_name)) = res {
                        log::debug!("insert(table: {table_name}, table_id: {table_id}): {e}")
                    } else {
                        log::debug!("insert(table_id: {table_id}): {e}")
                    }
                }
            })?;

        Ok(ret)
    }

    /*
    #[tracing::instrument(skip_all)]
    pub fn delete_pk(&self, table_id: u32, buffer: &[u8]) -> Result<(), NodesError> {
        self.measure(table_id, &INSTANCE_ENV_DELETE_PK);

        // Decode the primary key.
        let primary_key = PrimaryKey::decode(&mut &buffer[..]).map_err(NodesError::DecodePrimaryKey)?;
        // TODO: Actually delete the primary key?
        Err(NodesError::PrimaryKeyNotFound(primary_key))
    }

    #[tracing::instrument(skip_all)]
    pub fn delete_value(&self, table_id: u32, buffer: &[u8]) -> Result<(), NodesError> {
        let measure = self.measure(table_id, &INSTANCE_ENV_DELETE_VALUE);

        let stdb = &*self.dbic.relational_db;
        let tx = &mut *self.get_tx()?;

        let schema = stdb.row_schema_for_table(tx, table_id)?;
        let row = ProductValue::decode(&schema, &mut &buffer[..]).map_err(NodesError::DecodeRow)?;

        let row_id = row.to_data_key();
        // todo: check that res is true, but for now it always is
        let res = stdb
            .delete_pk(tx, table_id, row_id)
            .inspect_err_(|e| log::error!("delete_value(table_id: {table_id}): {e}"))?;

        self.with_trace_log(|l| {
            l.delete_value(
                measure.start_instant.unwrap(),
                measure.elapsed(),
                table_id,
                buffer.into(),
                res,
            )
        });

        if res {
            Ok(())
        } else {
            Err(NodesError::PrimaryKeyNotFound(PrimaryKey { data_key: row_id }))
        }
    }
    */

    /// Deletes all rows in the table identified by `table_id`
    /// where the column identified by `cols` equates to `value`.
    ///
    /// Returns an error if no columns were deleted or if the column wasn't found.
    #[tracing::instrument(skip_all)]
    pub fn delete_by_col_eq(&self, table_id: u32, col_id: u32, value: &[u8]) -> Result<u32, NodesError> {
        let stdb = &*self.dbic.relational_db;
        let tx = &mut *self.get_tx()?;

        // Interpret the `value` using the schema of the column.
        let eq_value = stdb.decode_column(tx, table_id, col_id, value)?;

        // Find all rows in the table where the column data equates to `value`.
        let seek = stdb.iter_by_col_eq(tx, table_id, col_id, eq_value)?;
        let seek = seek.map(|x| stdb.data_to_owned(x).into()).collect::<Vec<_>>();

        // Delete them and count how many we deleted and error if none.
        let count = stdb
            .delete_by_rel(tx, table_id, seek)
            .inspect_err_(|e| log::error!("delete_by_col_eq(table_id: {table_id}): {e}"))?
            .ok_or(NodesError::ColumnValueNotFound)?;

        Ok(count)
    }

    /*
    #[tracing::instrument(skip_all)]
    pub fn delete_range(
        &self,
        table_id: u32,
        cols: u32,
        start_buffer: &[u8],
        end_buffer: &[u8],
    ) -> Result<u32, NodesError> {
        let measure = self.measure(table_id, &INSTANCE_ENV_DELETE_RANGE);

        let stdb = &*self.dbic.relational_db;
        let tx = &mut *self.get_tx()?;

        let col_type = stdb.schema_for_column(tx, table_id, cols)?;

        let decode = |b: &[u8]| AlgebraicValue::decode(&col_type, &mut &b[..]).map_err(NodesError::DecodeValue);
        let start = decode(start_buffer)?;
        let end = decode(end_buffer)?;

        let range = stdb.range_scan(tx, table_id, cols, start..end)?;
        let range = range.map(|x| stdb.data_to_owned(x).into()).collect::<Vec<_>>();

        let count = stdb.delete_in(tx, table_id, range)?.ok_or(NodesError::RangeNotFound)?;

        self.with_trace_log(|l| {
            l.delete_range(
                measure.start_instant.unwrap(),
                measure.elapsed(),
                table_id,
                cols,
                start_buffer.into(),
                end_buffer.into(),
                count,
            )
        });

        Ok(count)
    }

    #[tracing::instrument(skip_all)]
    pub fn create_table(&self, _table_name: &str, _schema_bytes: &[u8]) -> Result<u32, NodesError> {
        // let now = SystemTime::now();

        // let stdb = &*self.dbic.relational_db;
        // let tx = &mut *self.get_tx()?;

        unimplemented!()
        // let schema = ProductType::decode(&mut &schema_bytes[..]).map_err(NodesError::DecodeSchema)?;

        // let table_id = stdb.create_table(tx, table_name, schema)?;

        // self.with_trace_log(|l| {
        //     l.create_table(
        //         now,
        //         now.elapsed().unwrap(),
        //         table_name.into(),
        //         schema_bytes.into(),
        //         table_id,
        //     );
        // });

        // Ok(table_id)
    }
    */

    /// Returns the `table_id` associated with the given `table_name`.
    ///
    /// Errors with `TableNotFound` if the table does not exist.
    #[tracing::instrument(skip_all)]
    pub fn get_table_id(&self, table_name: SatsString) -> Result<u32, NodesError> {
        let stdb = &*self.dbic.relational_db;
        let tx = &mut *self.get_tx()?;

        // Query the table id from the name.
        stdb.table_id_from_name(tx, table_name)?
            .ok_or(NodesError::TableNotFound)
    }

    /// Creates an index of type `index_type`,
    /// on a product of the given columns in `col_ids`,
    /// in the table identified by `table_id`.
    ///
    /// Currently only btree index type are supported.
    ///
    /// The `table_name` is used together with the column ids to construct the name of the index.
    /// As only single-column-indices are supported right now,
    /// the name will be in the format `{table_name}_{cols}`.
    #[tracing::instrument(skip_all)]
    pub fn create_index(
        &self,
        index_name: SatsString,
        table_id: u32,
        index_type: u8,
        col_ids: Vec<u8>,
    ) -> Result<(), NodesError> {
        let stdb = &*self.dbic.relational_db;
        let tx = &mut *self.get_tx()?;

        // TODO(george) This check should probably move towards src/db/index, but right
        // now the API is pretty hardwired towards btrees.
        //
        // TODO(george) Dedup the constant here.
        match index_type {
            0 => (),
            1 => todo!("Hash indexes not yet supported"),
            _ => return Err(NodesError::BadIndexType(index_type)),
        };

        let cols = NonEmpty::from_slice(&col_ids)
            .expect("Attempt to create an index with zero columns")
            .map(|x| x as u32);

        let is_unique = stdb.column_attrs(tx, table_id, &cols)?.is_unique();

        let index = IndexDef {
            table_id,
            cols,
            name: index_name.clone(),
            is_unique,
        };

        stdb.create_index(tx, index)?;

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
    pub fn iter_by_col_eq(&self, table_id: u32, col_id: u32, value: &[u8]) -> Result<Vec<u8>, NodesError> {
        let stdb = &*self.dbic.relational_db;
        let tx = &mut *self.get_tx()?;

        // Interpret the `value` using the schema of the column.
        let value = stdb.decode_column(tx, table_id, col_id, value)?;

        // Find all rows in the table where the column data matches `value`.
        // Concatenate and return these rows using bsatn encoding.
        let results = stdb.iter_by_col_eq(tx, table_id, col_id, value)?;
        let mut bytes = Vec::new();
        for result in results {
            bsatn::to_writer(&mut bytes, result.view()).unwrap();
        }
        Ok(bytes)
    }

    #[tracing::instrument(skip_all)]
    pub fn iter(&self, table_id: u32) -> impl Iterator<Item = Result<Vec<u8>, NodesError>> {
        use genawaiter::{sync::gen, yield_, GeneratorState};

        // Cheap Arc clones to untie the returned iterator from our own lifetime.
        let relational_db = self.dbic.relational_db.clone();
        let tx = self.tx.clone();

        // For now, just send buffers over a certain fixed size.
        fn should_yield_buf(buf: &Vec<u8>) -> bool {
            const SIZE: usize = 64 * 1024;
            buf.len() >= SIZE
        }

        let mut generator = Some(gen!({
            let stdb = &*relational_db;
            let tx = &mut *tx.get()?;

            let mut buf = Vec::new();
            let schema = stdb.row_schema_for_table(tx, table_id)?;
            schema.encode(&mut buf);
            yield_!(buf);

            let mut buf = Vec::new();
            for row in stdb.iter(tx, table_id)? {
                if should_yield_buf(&buf) {
                    yield_!(buf);
                    buf = Vec::new();
                }
                row.view().encode(&mut buf);
            }
            if !buf.is_empty() {
                yield_!(buf)
            }

            Ok(())
        }));

        std::iter::from_fn(move || match generator.as_mut()?.resume() {
            GeneratorState::Yielded(bytes) => Some(Ok(bytes)),
            GeneratorState::Complete(res) => {
                generator = None;
                match res {
                    Ok(()) => None,
                    Err(err) => Some(Err(err)),
                }
            }
        })
    }

    #[tracing::instrument(skip_all)]
    pub fn iter_filtered(&self, table_id: u32, filter: &[u8]) -> Result<impl Iterator<Item = Vec<u8>>, NodesError> {
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

        let schema = stdb.schema_for_table(tx, table_id)?;
        let row_type = ProductType::from(&schema);
        let filter = filter::Expr::from_bytes(
            // TODO: looks like module typespace is currently not hooked up to instances;
            // use empty typespace for now which should be enough for primitives
            // but figure this out later
            &Typespace::default(),
            &row_type.elements,
            filter,
        )
        .map_err(NodesError::DecodeFilter)?;
        let q = spacetimedb_vm::dsl::query(&schema).with_select(filter_to_column_op(&schema.table_name, filter));
        //TODO: How pass the `caller` here?
        let p = &mut DbProgram::new(stdb, tx, AuthCtx::for_current(self.dbic.identity));
        let results = match spacetimedb_vm::eval::run_ast(p, q.into()) {
            Code::Table(table) => table,
            _ => unreachable!("query should always return a table"),
        };
        Ok(std::iter::once(bsatn::to_vec(&row_type))
            .chain(results.data.into_iter().map(|row| bsatn::to_vec(&row.data)))
            .map(|bytes| bytes.expect("encoding algebraic values should never fail")))
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
