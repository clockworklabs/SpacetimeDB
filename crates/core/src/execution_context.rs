use std::sync::Arc;

use derive_more::Display;
use parking_lot::RwLock;
use spacetimedb_lib::Address;
use spacetimedb_primitives::TableId;

use crate::db::db_metrics::DB_METRICS;

pub enum MetricType {
    IndexSeeks,
    KeysScanned,
    RowsFetched,
    RowsInserted,
    RowsDeleted,
}
#[derive(Default, Clone)]
struct BufferMetric {
    pub table_id: TableId,
    pub index_seeks: u64,
    pub keys_scanned: u64,
    pub rows_fetched: u64,
    pub rows_inserted: u64,
    pub rows_deleted: u64,
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
            MetricType::RowsInserted => {
                self.rows_inserted += val;
            }
            MetricType::RowsDeleted => {
                self.rows_deleted += val;
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
    pub fn new() -> Self {
        Self(Vec::new())
    }

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
            flush_metric!(DB_METRICS.rdb_num_rows_inserted, metric, rows_inserted);
            flush_metric!(DB_METRICS.rdb_num_rows_deleted, metric, rows_deleted);
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
    reducer: Option<String>,
    /// The type of workload that is being executed.
    workload: WorkloadType,
    /// The Metrics to be reported for this transaction.
    pub metrics: Arc<RwLock<Metrics>>,
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
    /// Returns an [ExecutionContext] for a reducer transaction.
    pub fn reducer(database: Address, name: String) -> Self {
        Self {
            database,
            reducer: Some(name),
            workload: WorkloadType::Reducer,
            metrics: Arc::new(RwLock::new(Metrics::default())),
        }
    }

    /// Returns an [ExecutionContext] for a one-off sql query.
    pub fn sql(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Sql,
            metrics: Arc::new(RwLock::new(Metrics::default())),
        }
    }

    /// Returns an [ExecutionContext] for an initial subscribe call.
    pub fn subscribe(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Subscribe,
            metrics: Arc::new(RwLock::new(Metrics::default())),
        }
    }

    /// Returns an [ExecutionContext] for a subscription update.
    pub fn incremental_update(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Update,
            metrics: Arc::new(RwLock::new(Metrics::default())),
        }
    }

    /// Returns an [ExecutionContext] for an internal database operation.
    pub fn internal(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Internal,
            metrics: Arc::new(RwLock::new(Metrics::default())),
        }
    }

    /// Returns the address of the database on which we are operating.
    #[inline]
    pub fn database(&self) -> Address {
        self.database
    }

    /// If this is a reducer context, returns the name of the reducer.
    #[inline]
    pub fn reducer_name(&self) -> &str {
        self.reducer.as_deref().unwrap_or_default()
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
        let reducer = self.reducer.as_deref().unwrap_or_default();
        self.metrics.write().flush(&workload, &database, reducer);
    }
}
