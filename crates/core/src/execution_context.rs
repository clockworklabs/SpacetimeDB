use std::{collections::HashMap, sync::Arc};

use derive_more::Display;
use parking_lot::{Mutex};
use spacetimedb_lib::Address;
use spacetimedb_primitives::TableId;

use crate::db::db_metrics::DB_METRICS;

#[derive(Default)]
pub struct RecordMetrics {
    pub rdb_num_index_seeks: HashMap<TableId, u64>,
    pub rdb_num_keys_scanned: HashMap<TableId, u64>,
    pub rdb_num_rows_fetched: HashMap<TableId, u64>,
    pub cache_table_name: HashMap<TableId, String>,
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
    pub metrics: Arc<Mutex<RecordMetrics>>,
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
            metrics: Arc::new(Mutex::new(RecordMetrics::default())),
        }
    }

    /// Returns an [ExecutionContext] for a one-off sql query.
    pub fn sql(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Sql,
            metrics: Arc::new(Mutex::new(RecordMetrics::default())),
        }
    }

    /// Returns an [ExecutionContext] for an initial subscribe call.
    pub fn subscribe(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Subscribe,
            metrics: Arc::new(Mutex::new(RecordMetrics::default())),
        }
    }

    /// Returns an [ExecutionContext] for a subscription update.
    pub fn incremental_update(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Update,
            metrics: Arc::new(Mutex::new(RecordMetrics::default())),
        }
    }

    /// Returns an [ExecutionContext] for an internal database operation.
    pub fn internal(database: Address) -> Self {
        Self {
            database,
            reducer: None,
            workload: WorkloadType::Internal,
            metrics: Arc::new(Mutex::new(RecordMetrics::default())),
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
        let reducer = self.reducer.unwrap_or_default().to_string();
        let metric = self.metrics.clone();
        log::info!("dropping execution context");
        tokio::task::spawn_blocking(move || {
            let mut tables = 0;
            let metrics = metric.lock();
            if !metrics.rdb_num_index_seeks.is_empty() {
                metrics.rdb_num_index_seeks.iter().for_each(|(table_id, count)| {
                    DB_METRICS
                        .rdb_num_index_seeks
                        .with_label_values(&workload, &database, &reducer, &table_id.0, &metrics.cache_table_name[&table_id])
                        .inc_by(*count);
                    tables+=1;
                });
            }
            if !metrics.rdb_num_keys_scanned.is_empty() {
                metrics.rdb_num_index_seeks.iter().for_each(|(table_id, count)| {
                    DB_METRICS
                        .rdb_num_index_seeks
                        .with_label_values(&workload, &database, &reducer, &table_id.0, &metrics.cache_table_name[&table_id])
                        .inc_by(*count);
                    tables+=1;
                });
            }
            if !metrics.rdb_num_rows_fetched.is_empty() {
                metrics.rdb_num_index_seeks.iter().for_each(|(table_id, count)| {
                    DB_METRICS
                        .rdb_num_index_seeks
                        .with_label_values(&workload, &database, &reducer, &table_id.0, &metrics.cache_table_name[&table_id])
                        .inc_by(*count);
                    tables+=1;
                });

                log::info!("dropping tables execution context {:?}", tables);
            }
        });
    }
}