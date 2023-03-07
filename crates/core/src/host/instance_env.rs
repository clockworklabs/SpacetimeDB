use bytes::Bytes;
use parking_lot::{Mutex, MutexGuard};
use spacetimedb_lib::{PrimaryKey, TupleDef, TupleValue, TypeValue};
use std::ops::DerefMut;
use std::sync::Arc;
use std::time::SystemTime;

use crate::database_logger::{BacktraceProvider, LogLevel, Record};
use crate::db::relational_db::{RelationalDB, WrapTxWrapper};
use crate::error::NodesError;
use crate::util::prometheus_handle::HistogramVecHandle;
use crate::util::ResultInspectExt;
use crate::worker_database_instance::WorkerDatabaseInstance;
use crate::worker_metrics::{
    INSTANCE_ENV_DELETE_EQ, INSTANCE_ENV_DELETE_PK, INSTANCE_ENV_DELETE_RANGE, INSTANCE_ENV_DELETE_VALUE,
    INSTANCE_ENV_INSERT,
};

use super::host_controller::Scheduler;
use super::timestamp::Timestamp;
use super::tracelog::instance_trace::TraceLog;
use super::ReducerArgs;

#[derive(Clone)]
pub struct InstanceEnv {
    pub worker_database_instance: WorkerDatabaseInstance,
    pub scheduler: Scheduler,
    pub tx: TxSlot,
    pub trace_log: Option<Arc<Mutex<TraceLog>>>,
}

#[derive(Clone, Default)]
pub struct TxSlot {
    inner: Arc<Mutex<Option<WrapTxWrapper>>>,
}

// pub enum

// Generic 'instance environment' delegated to from various host types.
impl InstanceEnv {
    pub fn new(
        worker_database_instance: WorkerDatabaseInstance,
        scheduler: Scheduler,
        trace_log: Option<Arc<Mutex<TraceLog>>>,
    ) -> Self {
        Self {
            worker_database_instance,
            scheduler,
            tx: TxSlot::default(),
            trace_log,
        }
    }

    pub fn schedule(&self, reducer: String, args: Bytes, time: Timestamp) {
        self.scheduler.schedule(
            self.worker_database_instance.database_instance_id,
            reducer,
            ReducerArgs::Bsatn(args),
            time,
        )
    }

    fn get_tx(&self) -> Result<impl DerefMut<Target = WrapTxWrapper> + '_, GetTxError> {
        self.tx.get()
    }

    pub fn console_log(&self, level: LogLevel, record: &Record, bt: &dyn BacktraceProvider) {
        self.worker_database_instance
            .logger
            .lock()
            .unwrap()
            .write(level, record, bt);
        log::trace!(
            "MOD({}): {}",
            self.worker_database_instance.address.to_abbreviated_hex(),
            record.message
        );
    }

    pub fn insert(&self, table_id: u32, buffer: &[u8]) -> Result<(), NodesError> {
        let mut measure = HistogramVecHandle::new(
            &INSTANCE_ENV_INSERT,
            vec![self.worker_database_instance.address.to_hex(), format!("{}", table_id)],
        );
        measure.start();
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let tx = &mut *self.get_tx()?;

        let schema = stdb
            .schema_for_table(tx, table_id)
            .unwrap()
            .ok_or(NodesError::TableNotFound)?;
        let row = RelationalDB::decode_row(&schema, &mut &buffer[..]).map_err(NodesError::DecodeRow)?;

        stdb.insert(tx, table_id, row)
            .inspect_err_(|e| log::error!("insert(table_id: {table_id}): {e}"))?;
        if let Some(trace_log) = &self.trace_log {
            trace_log.lock().insert(
                measure.start_instant.unwrap(),
                measure.elapsed(),
                table_id,
                buffer.into(),
            );
        }
        Ok(())
    }

    pub fn delete_pk(&self, table_id: u32, buffer: &[u8]) -> Result<(), NodesError> {
        let mut measure = HistogramVecHandle::new(
            &INSTANCE_ENV_DELETE_PK,
            vec![self.worker_database_instance.address.to_hex(), format!("{}", table_id)],
        );
        measure.start();
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let tx = &mut *self.get_tx()?;

        let primary_key = PrimaryKey::decode(&mut &buffer[..]).map_err(NodesError::DecodePrimaryKey)?;
        // todo: check that res is true, but for now it always is
        let _res = stdb
            .delete_pk(tx, table_id, primary_key)
            .inspect_err_(|e| log::error!("delete_pk(table_id: {table_id}): {e}"))?
            .ok_or(NodesError::PrimaryKeyNotFound(primary_key))?;

        if let Some(trace_log) = &self.trace_log {
            trace_log.lock().delete_pk(
                measure.start_instant.unwrap(),
                measure.elapsed(),
                table_id,
                buffer.into(),
                _res,
            );
        }

        Ok(())
    }

    pub fn delete_value(&self, table_id: u32, buffer: &[u8]) -> Result<(), NodesError> {
        let mut measure = HistogramVecHandle::new(
            &INSTANCE_ENV_DELETE_VALUE,
            vec![self.worker_database_instance.address.to_hex(), format!("{}", table_id)],
        );
        measure.start();
        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let tx = &mut *self.get_tx()?;

        let schema = stdb
            .schema_for_table(tx, table_id)
            .unwrap()
            .ok_or(NodesError::TableNotFound)?;
        let row = RelationalDB::decode_row(&schema, &mut &buffer[..]).map_err(NodesError::DecodeRow)?;

        let pk = RelationalDB::pk_for_row(&row);
        // todo: check that res is true, but for now it always is
        let _res = stdb
            .delete_pk(tx, table_id, pk)
            .inspect_err_(|e| log::error!("delete_value(table_id: {table_id}): {e}"))?
            .ok_or(NodesError::PrimaryKeyNotFound(pk))?;
        if let Some(trace_log) = &self.trace_log {
            trace_log.lock().delete_value(
                measure.start_instant.unwrap(),
                measure.elapsed(),
                table_id,
                buffer.into(),
                _res,
            );
        }
        Ok(())
    }

    pub fn delete_eq(&self, table_id: u32, col_id: u32, buffer: &[u8]) -> Result<u32, NodesError> {
        let mut measure = HistogramVecHandle::new(
            &INSTANCE_ENV_DELETE_EQ,
            vec![self.worker_database_instance.address.to_hex(), format!("{}", table_id)],
        );
        measure.start();

        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let tx = &mut *self.get_tx()?;

        let schema = stdb
            .schema_for_table(tx, table_id)
            .unwrap()
            .ok_or(NodesError::TableNotFound)?;
        let type_def = &schema
            .elements
            .get(col_id as usize)
            .ok_or(NodesError::BadColumn)?
            .algebraic_type;

        let eq_value = TypeValue::decode(type_def, &mut &buffer[..]).map_err(NodesError::DecodeValue)?;
        let seek = stdb.seek(tx, table_id, col_id, eq_value)?;
        let seek: Vec<TupleValue> = seek.collect::<Vec<_>>();
        let count = stdb
            .delete_in(tx, table_id, seek)
            .inspect_err_(|e| log::error!("delete_eq(table_id: {table_id}): {e}"))?
            .ok_or(NodesError::ColumnValueNotFound)?;

        if let Some(trace_log) = &self.trace_log {
            trace_log.lock().delete_eq(
                measure.start_instant.unwrap(),
                measure.elapsed(),
                table_id,
                col_id,
                buffer.into(),
                count,
            );
        }

        Ok(count)
    }

    pub fn delete_range(
        &self,
        table_id: u32,
        col_id: u32,
        start_buffer: &[u8],
        end_buffer: &[u8],
    ) -> Result<u32, NodesError> {
        let mut measure = HistogramVecHandle::new(
            &INSTANCE_ENV_DELETE_RANGE,
            vec![self.worker_database_instance.address.to_hex(), format!("{}", table_id)],
        );
        measure.start();

        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let tx = &mut *self.get_tx()?;

        let schema = stdb.schema_for_table(tx, table_id).unwrap().unwrap();
        let col_type = &schema
            .elements
            .get(col_id as usize)
            .ok_or(NodesError::BadColumn)?
            .algebraic_type;

        let decode = |b: &[u8]| TypeValue::decode(col_type, &mut &b[..]).map_err(NodesError::DecodeValue);
        let start = decode(start_buffer)?;
        let end = decode(end_buffer)?;

        let range = stdb.range_scan(tx, table_id, col_id, start..end)?;
        let range = range.collect::<Vec<_>>();

        let count = stdb.delete_in(tx, table_id, range)?.ok_or(NodesError::RangeNotFound)?;
        if let Some(trace_log) = &self.trace_log {
            trace_log.lock().delete_range(
                measure.start_instant.unwrap(),
                measure.elapsed(),
                table_id,
                col_id,
                start_buffer.into(),
                end_buffer.into(),
                count,
            );
        }
        Ok(count)
    }

    pub fn create_table(&self, table_name: &str, schema_bytes: &[u8]) -> Result<u32, NodesError> {
        let now = SystemTime::now();

        let mut stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let tx = &mut *self.get_tx()?;

        let schema = TupleDef::decode(&mut &schema_bytes[..]).map_err(NodesError::DecodeSchema)?;

        let table_id = stdb.create_table(tx, table_name, schema)?;

        if let Some(trace_log) = &self.trace_log {
            trace_log.lock().create_table(
                now,
                now.elapsed().unwrap(),
                table_name.into(),
                schema_bytes.into(),
                table_id,
            );
        }

        Ok(table_id)
    }

    pub fn get_table_id(&self, table_name: &str) -> Result<u32, NodesError> {
        let now = SystemTime::now();

        let stdb = self.worker_database_instance.relational_db.lock().unwrap();
        let tx = &mut *self.get_tx()?;

        let table_id = stdb
            .table_id_from_name(tx, table_name)?
            .ok_or(NodesError::TableNotFound)?;
        if let Some(trace_log) = &self.trace_log {
            trace_log
                .lock()
                .get_table_id(now, now.elapsed().unwrap(), table_name.into(), table_id);
        }
        Ok(table_id)
    }

    pub fn iter(&self, table_id: u32) -> impl Iterator<Item = Result<Vec<u8>, NodesError>> + Send {
        use genawaiter::{sync::gen, yield_, GeneratorState};

        // Cheap Arc clones to untie the returned iterator from our own lifetime.
        let relational_db = self.worker_database_instance.relational_db.clone();
        let tx = self.tx.clone();

        let mut generator = gen!({
            let stdb = relational_db.lock().unwrap();
            let mut tx = &mut *tx.get()?;

            let mut bytes = Vec::new();
            let schema = stdb
                .schema_for_table(&mut tx, table_id)?
                .ok_or(NodesError::TableNotFound)?;
            schema.encode(&mut bytes);
            yield_!(bytes);

            let mut count = 0;
            for row_bytes in stdb.scan_raw(&mut tx, table_id)? {
                count += 1;
                yield_!(row_bytes);
            }

            Ok(())
        });

        std::iter::from_fn(move || match generator.resume() {
            GeneratorState::Yielded(bytes) => Some(Ok(bytes)),
            GeneratorState::Complete(Err(err)) => Some(Err(err)),
            GeneratorState::Complete(Ok(())) => None,
        })
    }
}

impl TxSlot {
    pub fn set<T>(&self, tx: WrapTxWrapper, f: impl FnOnce() -> T) -> (WrapTxWrapper, T) {
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

    pub fn get(&self) -> Result<impl DerefMut<Target = WrapTxWrapper> + '_, GetTxError> {
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
