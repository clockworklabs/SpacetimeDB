use spacetimedb_lib::Identity;
use spacetimedb_query::metrics::QueryMetrics;

use crate::{db::db_metrics::DB_METRICS, execution_context::WorkloadType};

pub mod delta;
pub mod execution_unit;
pub mod module_subscription_actor;
pub mod module_subscription_manager;
pub mod query;
#[allow(clippy::module_inception)] // it's right this isn't ideal :/
pub mod subscription;
pub mod tx;

pub(crate) fn record_query_metrics(workload: WorkloadType, db: &Identity, metrics: QueryMetrics) {
    DB_METRICS
        .rdb_num_index_seeks
        .with_label_values(&workload, db)
        .inc_by(metrics.index_seeks as u64);
    DB_METRICS
        .rdb_num_rows_scanned
        .with_label_values(&workload, db)
        .inc_by(metrics.rows_scanned as u64);
}
