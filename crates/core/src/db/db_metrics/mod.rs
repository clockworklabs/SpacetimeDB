use crate::execution_context::WorkloadType;
use once_cell::sync::Lazy;
use prometheus::{HistogramVec, IntCounterVec, IntGaugeVec};
use spacetimedb_lib::Identity;
use spacetimedb_metrics::metrics_group;
use spacetimedb_primitives::TableId;

metrics_group!(
    #[non_exhaustive]
    pub struct DbMetrics {
        #[name = spacetime_num_table_rows]
        #[help = "The number of rows in a table"]
        #[labels(db: Identity, table_id: u32, table_name: str)]
        pub rdb_num_table_rows: IntGaugeVec,

        #[name = spacetime_num_rows_inserted_total]
        #[help = "The cumulative number of rows inserted into a table"]
        #[labels(txn_type: WorkloadType, db: Identity, reducer_or_query: str, table_id: u32, table_name: str)]
        pub rdb_num_rows_inserted: IntCounterVec,

        #[name = spacetime_num_rows_deleted_total]
        #[help = "The cumulative number of rows deleted from a table"]
        #[labels(txn_type: WorkloadType, db: Identity, reducer_or_query: str, table_id: u32, table_name: str)]
        pub rdb_num_rows_deleted: IntCounterVec,

        #[name = spacetime_num_rows_fetched_total]
        #[help = "The cumulative number of rows fetched from a table"]
        #[labels(txn_type: WorkloadType, db: Identity, reducer_or_query: str, table_id: u32, table_name: str)]
        pub rdb_num_rows_fetched: IntCounterVec,

        #[name = spacetime_num_index_keys_scanned_total]
        #[help = "The cumulative number of keys scanned from an index"]
        #[labels(txn_type: WorkloadType, db: Identity, reducer_or_query: str, table_id: u32, table_name: str)]
        pub rdb_num_keys_scanned: IntCounterVec,

        #[name = spacetime_num_index_seeks_total]
        #[help = "The cumulative number of index seeks"]
        #[labels(txn_type: WorkloadType, db: Identity, reducer_or_query: str, table_id: u32, table_name: str)]
        pub rdb_num_index_seeks: IntCounterVec,

        #[name = spacetime_num_txns_total]
        #[help = "The cumulative number of transactions, including both commits and rollbacks"]
        #[labels(txn_type: WorkloadType, db: Identity, reducer: str, committed: bool)]
        pub rdb_num_txns: IntCounterVec,

        #[name = spacetime_txn_elapsed_time_sec]
        #[help = "The total elapsed (wall) time of a transaction (in seconds)"]
        #[labels(txn_type: WorkloadType, db: Identity, reducer: str)]
        // Prometheus histograms have default buckets,
        // which broadly speaking,
        // are tailored to measure the response time of a network service.
        //
        // However we expect a different value distribution for OLTP workloads.
        // In particular the smallest bucket value is 5ms by default.
        // But we expect many transactions to be on the order of microseconds.
        #[buckets(10e-6, 50e-6, 100e-6, 500e-6, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1, 5, 10)]
        pub rdb_txn_elapsed_time_sec: HistogramVec,

        #[name = spacetime_txn_cpu_time_sec]
        #[help = "The time spent executing a transaction (in seconds), excluding time spent waiting to acquire database locks"]
        #[labels(txn_type: WorkloadType, db: Identity, reducer: str)]
        // Prometheus histograms have default buckets,
        // which broadly speaking,
        // are tailored to measure the response time of a network service.
        //
        // However we expect a different value distribution for OLTP workloads.
        // In particular the smallest bucket value is 5ms by default.
        // But we expect many transactions to be on the order of microseconds.
        #[buckets(10e-6, 50e-6, 100e-6, 500e-6, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1, 5, 10)]
        pub rdb_txn_cpu_time_sec: HistogramVec,

        #[name = spacetime_message_log_size_bytes]
        #[help = "For a given database, the number of bytes occupied by its message log"]
        #[labels(db: Identity)]
        pub message_log_size: IntGaugeVec,

        #[name = spacetime_module_log_file_size_bytes]
        #[help = "For a given module, the size of its log file (in bytes)"]
        #[labels(db: Identity)]
        pub module_log_file_size: IntGaugeVec,

        #[name = spacetime_table_size_bytes]
        #[help = "The number of bytes in a table with the precision of a page size"]
        #[labels(db: Identity, table_id: u32, table_name: str)]
        pub rdb_table_size: IntGaugeVec,
    }
);

pub static DB_METRICS: Lazy<DbMetrics> = Lazy::new(DbMetrics::new);

/// Returns the number of committed rows in the table named by `table_name` and identified by `table_id` in the database `db_address`.
pub fn table_num_rows(db_identity: Identity, table_id: TableId, table_name: &str) -> u64 {
    DB_METRICS
        .rdb_num_table_rows
        .with_label_values(&db_identity, &table_id.0, table_name)
        .get() as _
}
