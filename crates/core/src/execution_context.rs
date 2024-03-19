use std::cell::RefCell;

use derive_more::Display;
use spacetimedb_lib::Address;
use spacetimedb_primitives::TableId;

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
            index_seeks: 0,
            keys_scanned: 0,
            rows_fetched: 0,
            cache_table_name: table_name,
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

    fn flush(&mut self, workload: &WorkloadType, database: &Address, reducer: &str) {
        self.0.iter().for_each(|metric| {
            DB_METRICS
                .rdb_num_index_seeks
                .with_label_values(
                    workload,
                    database,
                    reducer,
                    &metric.table_id.0,
                    &metric.cache_table_name,
                )
                .inc_by(metric.index_seeks);

            DB_METRICS
                .rdb_num_keys_scanned
                .with_label_values(
                    workload,
                    database,
                    reducer,
                    &metric.table_id.0,
                    &metric.cache_table_name,
                )
                .inc_by(metric.keys_scanned);

            DB_METRICS
                .rdb_num_rows_fetched
                .with_label_values(
                    workload,
                    database,
                    reducer,
                    &metric.table_id.0,
                    &metric.cache_table_name,
                )
                .inc_by(metric.keys_scanned);
        });
    }
}

/// Represents the context under which a database runtime method is executed.
/// In particular it provides details about the currently executing txn to runtime operations.
/// More generally it acts as a container for information that database operations may require to function correctly.
#[derive(Default, Clone)]
pub struct ExecutionContext<'a> {
    /// The database on which a transaction is being executed.
    database: Address,
    /// The reducer from which the current transaction originated.
    reducer: Option<&'a str>,
    /// The type of workload that is being executed.
    workload: WorkloadType,
    /// The Metrics to be reported for this transaction.
    pub metrics: RefCell<Metrics>,
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

impl<'a> ExecutionContext<'a> {
    /// Returns an [ExecutionContext] for a reducer transaction.
    pub fn reducer(database: Address, name: &'a str) -> Self {
        Self {
            database,
            reducer: Some(name),
            workload: WorkloadType::Reducer,
            metrics: RefCell::new(Metrics::default()),
        }
    }

    /// Returns an [ExecutionContext] for a one-off sql query.
    pub fn sql(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Sql,
            metrics: RefCell::new(Metrics::default()),
        }
    }

    /// Returns an [ExecutionContext] for an initial subscribe call.
    pub fn subscribe(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Subscribe,
            metrics: RefCell::new(Metrics::default()),
        }
    }

    /// Returns an [ExecutionContext] for a subscription update.
    pub fn incremental_update(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Update,
            metrics: RefCell::new(Metrics::default()),
        }
    }

    /// Returns an [ExecutionContext] for an internal database operation.
    pub fn internal(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Internal,
            metrics: RefCell::new(Metrics::default()),
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
        self.reducer.unwrap_or_default()
    }

    /// Returns the type of workload that is being executed.
    #[inline]
    pub fn workload(&self) -> WorkloadType {
        self.workload
    }
}

impl Drop for ExecutionContext<'_> {
    fn drop(&mut self) {
        let workload = self.workload;
        let database = self.database;
        let reducer = self.reducer.unwrap_or_default();
        self.metrics.borrow_mut().flush(&workload, &database, reducer);
    }
}
