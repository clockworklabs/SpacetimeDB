use std::sync::Arc;

use bytes::Bytes;
use derive_more::Display;
use parking_lot::RwLock;
use spacetimedb_commitlog::{payload::txdata, Varchar};
use spacetimedb_lib::{Address, Identity, Timestamp};
use spacetimedb_primitives::TableId;
use spacetimedb_sats::bsatn;
use crate::db::db_metrics::DB_METRICS;

pub enum MetricType {
    IndexSeeks,
    KeysScanned,
    RowsFetched,
}

#[derive(Default, Clone)]
struct BufferMetric {
    pub table_id: TableId,
    pub index_seeks: u64,
    pub keys_scanned: u64,
    pub rows_fetched: u64,
    pub cache_table_name: String,
}

impl BufferMetric {
    pub fn inc_by(&mut self, ty: MetricType, val: u64) {
        match ty {
            MetricType::IndexSeeks => {
                self.index_seeks += val;
            }
            MetricType::KeysScanned => {
                self.keys_scanned += val;
            }
            MetricType::RowsFetched => {
                self.rows_fetched += val;
            }
        }
    }
}

impl BufferMetric {
    pub fn new(table_id: TableId, table_name: String) -> Self {
        Self {
            table_id,
            cache_table_name: table_name,
            ..Default::default()
        }
    }
}

#[derive(Default, Clone)]
pub struct Metrics(Vec<BufferMetric>);

impl Metrics {
    pub fn inc_by<F: FnOnce() -> String>(&mut self, table_id: TableId, ty: MetricType, val: u64, get_table_name: F) {
        if let Some(metric) = self.0.iter_mut().find(|x| x.table_id == table_id) {
            metric.inc_by(ty, val);
        } else {
            let table_name = get_table_name();
            let mut metric = BufferMetric::new(table_id, table_name);
            metric.inc_by(ty, val);
            self.0.push(metric);
        }
    }

    pub fn table_exists(&self, table_id: TableId) -> bool {
        self.0.iter().any(|x| x.table_id == table_id)
    }

    #[allow(dead_code)]
    fn flush(&mut self, workload: &WorkloadType, database: &Address, reducer: &str) {
        macro_rules! flush_metric {
            ($db_metric:expr, $metric:expr, $metric_field:ident) => {
                if $metric.$metric_field > 0 {
                    $db_metric
                        .with_label_values(
                            workload,
                            database,
                            reducer,
                            &$metric.table_id.0,
                            &$metric.cache_table_name,
                        )
                        .inc_by($metric.$metric_field);
                }
            };
        }

        self.0.iter().for_each(|metric| {
            flush_metric!(DB_METRICS.rdb_num_index_seeks, metric, index_seeks);
            flush_metric!(DB_METRICS.rdb_num_keys_scanned, metric, keys_scanned);
            flush_metric!(DB_METRICS.rdb_num_rows_fetched, metric, rows_fetched);
        });
    }
}

/// Represents the context under which a database runtime method is executed.
/// In particular it provides details about the currently executing txn to runtime operations.
/// More generally it acts as a container for information that database operations may require to function correctly.
#[derive(Default, Clone)]
pub struct ExecutionContext {
    /// The database on which a transaction is being executed.
    database: Address,
    /// The reducer from which the current transaction originated.
    reducer: Option<ReducerContext>,
    /// The type of workload that is being executed.
    workload: WorkloadType,
    /// The Metrics to be reported for this transaction.
    pub metrics: Arc<RwLock<Metrics>>,
}

/// If an [`ExecutionContext`] is a reducer context, describes the reducer.
///
/// Note that this information is written to persistent storage.
#[derive(Clone)]
pub struct ReducerContext {
    /// The name of the reducer.
    pub name: String,
    /// The [`Identity`] of the caller.
    pub caller_identity: Identity,
    /// The [`Address`] of the caller.
    pub caller_address: Address,
    /// The timestamp of the reducer invocation.
    pub timestamp: Timestamp,
    /// The BSATN-encoded arguments given to the reducer.
    ///
    /// Note that [`Bytes`] is a refcounted value, but the memory it points to
    /// can be large-ish. The reference should be freed as soon as possible.
    pub arg_bsatn: Bytes,
}

impl From<&ReducerContext> for txdata::Inputs {
    fn from(
        ReducerContext {
            name,
            caller_identity,
            caller_address,
            timestamp,
            arg_bsatn,
        }: &ReducerContext,
    ) -> Self {
        let reducer_name = Arc::new(Varchar::from_str_truncate(name));
        let cap = arg_bsatn.len()
        /* caller_identity */
        + 32
        /* caller_address */
        + 16
        /* timestamp */
        + 8;
        let mut buf = Vec::with_capacity(cap);
        bsatn::to_writer(&mut buf, caller_identity).unwrap();
        bsatn::to_writer(&mut buf, caller_address).unwrap();
        bsatn::to_writer(&mut buf, timestamp).unwrap();
        buf.extend_from_slice(arg_bsatn);

        txdata::Inputs {
            reducer_name,
            reducer_args: buf.into(),
        }
    }
}

/// Classifies a transaction according to its workload.
/// A transaction can be executing a reducer.
/// It can be used to satisfy a one-off sql query or subscription.
/// It can also be an internal operation that is not associated with a reducer or sql request.
#[derive(Clone, Copy, Display, Hash, PartialEq, Eq, strum::AsRefStr)]
pub enum WorkloadType {
    Reducer,
    Sql,
    Subscribe,
    Update,
    Internal,
}

impl Default for WorkloadType {
    fn default() -> Self {
        Self::Internal
    }
}

impl ExecutionContext {
    /// Returns an [ExecutionContext] with the provided parameters and empty metrics.
    fn new(database: Address, reducer: Option<ReducerContext>, workload: WorkloadType) -> Self {
        Self {
            database,
            reducer,
            workload,
            metrics: <_>::default(),
        }
    }

    /// Returns an [ExecutionContext] for a reducer transaction.
    pub fn reducer(database: Address, ctx: ReducerContext) -> Self {
        Self::new(database, Some(ctx), WorkloadType::Reducer)
    }

    /// Returns an [ExecutionContext] for a one-off sql query.
    pub fn sql(database: Address) -> Self {
        Self::new(database, None, WorkloadType::Sql)
    }

    /// Returns an [ExecutionContext] for an initial subscribe call.
    pub fn subscribe(database: Address) -> Self {
        Self::new(database, None, WorkloadType::Subscribe)
    }

    /// Returns an [ExecutionContext] for a subscription update.
    pub fn incremental_update(database: Address) -> Self {
        Self::new(database, None, WorkloadType::Update)
    }

    /// Returns an [ExecutionContext] for an internal database operation.
    pub fn internal(database: Address) -> Self {
        Self::new(database, None, WorkloadType::Internal)
    }

    /// Returns the address of the database on which we are operating.
    #[inline]
    pub fn database(&self) -> Address {
        self.database
    }

    /// If this is a reducer context, returns the name of the reducer.
    #[inline]
    pub fn reducer_name(&self) -> &str {
        self.reducer.as_ref().map(|ctx| ctx.name.as_str()).unwrap_or_default()
    }

    /// If this is a reducer context, returns the full reducer metadata.
    #[inline]
    pub fn reducer_context(&self) -> Option<&ReducerContext> {
        self.reducer.as_ref()
    }

    /// Returns the type of workload that is being executed.
    #[inline]
    pub fn workload(&self) -> WorkloadType {
        self.workload
    }
}

impl Drop for ExecutionContext {
    fn drop(&mut self) {
        let workload = self.workload;
        let database = self.database;
        let reducer = self.reducer_name();
        self.metrics.write().flush(&workload, &database, reducer);
    }
}
