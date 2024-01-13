use std::{collections::HashMap, sync::Mutex};

use crate::execution_context::WorkloadType;
use crate::host::AbiCall;
use once_cell::sync::Lazy;
use prometheus::{GaugeVec, HistogramVec, IntCounterVec, IntGaugeVec};
use spacetimedb_lib::Address;
use spacetimedb_metrics::metrics_group;

metrics_group!(
    #[non_exhaustive]
    pub struct DbMetrics {
        #[name = spacetime_rdb_drop_table_time]
        #[help = "The time spent dropping a table"]
        #[labels(table_id: u32)]
        pub rdb_drop_table_time: HistogramVec,

        #[name = spacetime_rdb_insert_row_time]
        #[help = "The time spent inserting into a table"]
        #[labels(table_id: u32)]
        pub rdb_insert_row_time: HistogramVec,

        #[name = spacetime_rdb_delete_in_time]
        #[help = "The time spent deleting values in a set from a table"]
        #[labels(table_id: u32)]
        pub rdb_delete_by_rel_time: HistogramVec,

        #[name = spacetime_num_table_rows]
        #[help = "The number of rows in a table"]
        #[labels(db: Address, table_id: u32, table_name: str)]
        pub rdb_num_table_rows: IntGaugeVec,

        #[name = spacetime_num_rows_inserted_cumulative]
        #[help = "The cumulative number of rows inserted into a table"]
        #[labels(txn_type: WorkloadType, db: Address, reducer_or_query: str, table_id: u32, table_name: str)]
        pub rdb_num_rows_inserted: IntCounterVec,

        #[name = spacetime_num_rows_deleted_cumulative]
        #[help = "The cumulative number of rows deleted from a table"]
        #[labels(txn_type: WorkloadType, db: Address, reducer_or_query: str, table_id: u32, table_name: str)]
        pub rdb_num_rows_deleted: IntCounterVec,

        #[name = spacetime_num_rows_fetched_cumulative]
        #[help = "The cumulative number of rows fetched from a table"]
        #[labels(txn_type: WorkloadType, db: Address, reducer_or_query: str, table_id: u32, table_name: str)]
        pub rdb_num_rows_fetched: IntCounterVec,

        #[name = spacetime_num_index_keys_scanned_cumulative]
        #[help = "The cumulative number of keys scanned from an index"]
        #[labels(txn_type: WorkloadType, db: Address, reducer_or_query: str, table_id: u32, table_name: str)]
        pub rdb_num_keys_scanned: IntCounterVec,

        #[name = spacetime_num_index_seeks_cumulative]
        #[help = "The cumulative number of index seeks"]
        #[labels(txn_type: WorkloadType, db: Address, reducer_or_query: str, table_id: u32, table_name: str)]
        pub rdb_num_index_seeks: IntCounterVec,

        #[name = spacetime_num_txns_cumulative]
        #[help = "The cumulative number of transactions, including both commits and rollbacks"]
        #[labels(txn_type: WorkloadType, db: Address, reducer: str, committed: bool)]
        pub rdb_num_txns: IntCounterVec,

        #[name = spacetime_txn_elapsed_time_sec]
        #[help = "The total elapsed (wall) time of a transaction (in seconds)"]
        #[labels(txn_type: WorkloadType, db: Address, reducer: str)]
        #[buckets(
            1e-6, 5e-6, 1e-5, 5e-5, 1e-4, 5e-4, 1e-3, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0
        )]
        pub rdb_txn_elapsed_time_sec: HistogramVec,

        #[name = spacetime_txn_cpu_time_sec]
        #[help = "The time spent executing a transaction (in seconds), excluding time spent waiting to acquire database locks"]
        #[labels(txn_type: WorkloadType, db: Address, reducer: str)]
        #[buckets(
            1e-6, 5e-6, 1e-5, 5e-5, 1e-4, 5e-4, 1e-3, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0
        )]
        pub rdb_txn_cpu_time_sec: HistogramVec,

        #[name = spacetime_txn_cpu_time_sec_max]
        #[help = "The cpu time of the longest running transaction (in seconds)"]
        #[labels(txn_type: WorkloadType, db: Address, reducer: str)]
        pub rdb_txn_cpu_time_sec_max: GaugeVec,

        #[name = spacetime_query_cpu_time_sec]
        #[help = "The time spent executing a query (in seconds)"]
        #[labels(txn_type: WorkloadType, db: Address)]
        #[buckets(
            1e-6, 5e-6, 1e-5, 5e-5, 1e-4, 5e-4, 1e-3, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0
        )]
        pub rdb_query_cpu_time_sec: HistogramVec,

        #[name = spacetime_query_cpu_time_sec_max]
        #[help = "The cpu time of the longest running query (in seconds)"]
        #[labels(txn_type: WorkloadType, db: Address)]
        pub rdb_query_cpu_time_sec_max: GaugeVec,

        #[name = spacetime_query_compile_time_sec]
        #[help = "The time spent compiling a query (in seconds)"]
        #[labels(txn_type: WorkloadType, db: Address)]
        #[buckets(
            1e-6, 5e-6, 1e-5, 5e-5, 1e-4, 5e-4, 1e-3, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0
        )]
        pub rdb_query_compile_time_sec: HistogramVec,

        #[name = spacetime_query_compile_time_sec_max]
        #[help = "The maximum query compilation time (in seconds)"]
        #[labels(txn_type: WorkloadType, db: Address)]
        pub rdb_query_compile_time_sec_max: GaugeVec,

        #[name = spacetime_wasm_abi_call_duration_sec]
        #[help = "The total duration of a spacetime wasm abi call (in seconds); includes row serialization and copying into wasm memory"]
        #[labels(db: Address, reducer: str, call: AbiCall)]
        #[buckets(
            1e-6, 5e-6, 1e-5, 5e-5, 1e-4, 5e-4, 1e-3, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0
        )]
        pub wasm_abi_call_duration_sec: HistogramVec,

        #[name = spacetime_message_log_size_bytes]
        #[help = "For a given database, the number of bytes occupied by its message log"]
        #[labels(db: Address)]
        pub message_log_size: IntGaugeVec,

        #[name = spacetime_object_db_disk_usage]
        #[help = "For a given database, the number of bytes occupied by large object storage"]
        #[labels(db: Address)]
        pub object_db_disk_usage: IntGaugeVec,

        #[name = spacetime_module_log_file_size_bytes]
        #[help = "For a given module, the size of its log file (in bytes)"]
        #[labels(db: Address)]
        pub module_log_file_size: IntGaugeVec,
    }
);

type ReducerLabel = (Address, WorkloadType, String);
type AddressLabel = (Address, WorkloadType);

pub static MAX_TX_CPU_TIME: Lazy<Mutex<HashMap<ReducerLabel, f64>>> = Lazy::new(|| Mutex::new(HashMap::new()));
pub static MAX_QUERY_CPU_TIME: Lazy<Mutex<HashMap<AddressLabel, f64>>> = Lazy::new(|| Mutex::new(HashMap::new()));
pub static MAX_QUERY_COMPILE_TIME: Lazy<Mutex<HashMap<AddressLabel, f64>>> = Lazy::new(|| Mutex::new(HashMap::new()));
pub static DB_METRICS: Lazy<DbMetrics> = Lazy::new(DbMetrics::new);

pub fn reset_counters() {
    // Reset max reducer durations
    DB_METRICS.rdb_txn_cpu_time_sec_max.0.reset();
    MAX_TX_CPU_TIME.lock().unwrap().clear();
    // Reset max query durations
    DB_METRICS.rdb_query_cpu_time_sec_max.0.reset();
    MAX_QUERY_CPU_TIME.lock().unwrap().clear();
}
