use serde::{Deserialize, Serialize};
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_lib::ProductType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlStmtResult<Row> {
    pub schema: ProductType,
    pub rows: Vec<Row>,
    pub total_duration_micros: u64,
    #[serde(default)]
    pub stats: SqlStmtStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SqlStmtStats {
    pub rows_inserted: u64,
    pub rows_deleted: u64,
    pub rows_updated: u64,
}

impl SqlStmtStats {
    pub fn from_metrics(metrics: &ExecutionMetrics) -> Self {
        Self {
            rows_inserted: metrics.rows_inserted,
            rows_deleted: metrics.rows_deleted,
            rows_updated: metrics.rows_updated,
        }
    }
}
