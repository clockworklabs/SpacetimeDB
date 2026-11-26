use spacetimedb_schema::table_name::TableName;
use spacetimedb_subscription::SubscriptionPlanMetrics;

/// Metrics data for a single subscription query execution
#[derive(Debug)]
pub struct QueryMetrics {
    pub scan_type: String,
    pub table_name: TableName,
    pub unindexed_columns: String,
    pub rows_scanned: u64,
    pub execution_time_micros: u64,
}

/// Analyzes subscription scan strategy and creates QueryMetrics.
///
/// `table_name` is the original table the subscription returns.
pub fn get_query_metrics(
    table_name: TableName,
    plan_metrics: &SubscriptionPlanMetrics,
    rows_scanned: u64,
    execution_time_micros: u64,
) -> QueryMetrics {
    QueryMetrics {
        scan_type: plan_metrics.scan_type().to_owned(),
        table_name: plan_metrics.filtered_table().cloned().unwrap_or(table_name),
        unindexed_columns: plan_metrics.unindexed_columns().to_owned(),
        rows_scanned,
        execution_time_micros,
    }
}
