use spacetimedb_lib::{metrics::ExecutionMetrics, Identity};

use crate::{db::db_metrics::DB_METRICS, execution_context::WorkloadType, worker_metrics::WORKER_METRICS};

pub mod delta;
pub mod execution_unit;
pub mod module_subscription_actor;
pub mod module_subscription_manager;
pub mod query;
#[allow(clippy::module_inception)] // it's right this isn't ideal :/
pub mod subscription;
pub mod tx;

/// Update the global system metrics with transaction-level execution metrics
pub(crate) fn record_exec_metrics(workload: &WorkloadType, db: &Identity, metrics: ExecutionMetrics) {
    DB_METRICS
        .rdb_num_index_seeks
        .with_label_values(workload, db)
        .inc_by(metrics.index_seeks as u64);
    DB_METRICS
        .rdb_num_rows_scanned
        .with_label_values(workload, db)
        .inc_by(metrics.rows_scanned as u64);
    DB_METRICS
        .rdb_num_bytes_scanned
        .with_label_values(workload, db)
        .inc_by(metrics.bytes_scanned as u64);
    DB_METRICS
        .rdb_num_bytes_written
        .with_label_values(workload, db)
        .inc_by(metrics.bytes_written as u64);
    WORKER_METRICS
        .bytes_sent_to_clients
        .with_label_values(workload, db)
        .inc_by(metrics.bytes_sent_to_clients as u64);
}
