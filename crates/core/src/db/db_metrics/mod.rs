use crate::{execution_context::TransactionType, util::typed_prometheus::metrics_group};
use once_cell::sync::Lazy;
use prometheus::{Histogram, HistogramVec, IntCounterVec};

metrics_group!(
    #[non_exhaustive]
    pub struct DbMetrics {
        #[name = spacetime_tdb_insert_time]
        #[help = "Time time it takes for the transactional store to perform an insert"]
        pub tdb_insert_time: Histogram,

        #[name = spacetime_tdb_delete_time]
        #[help = "Time time it takes for the transactional store to perform a delete"]
        pub tdb_delete_time: Histogram,

        #[name = spacetime_tdb_seek_time]
        #[help = "Time time it takes for the transactional store to perform a seek"]
        pub tdb_seek_time: Histogram,

        #[name = spacetime_tdb_scan_time]
        #[help = "Time time it takes for the transactional store to perform a scan"]
        pub tdb_scan_time: Histogram,

        #[name = spacetime_tdb_commit_time]
        #[help = "Time time it takes for the transactional store to perform a Tx commit"]
        pub tdb_commit_time: Histogram,

        #[name = spacetime_rdb_create_table_time]
        #[help = "The time it takes to create a table"]
        #[labels(table_name: str)]
        pub rdb_create_table_time: HistogramVec,

        #[name = spacetime_rdb_drop_table_time]
        #[help = "The time spent dropping a table"]
        #[labels(table_id: u32)]
        pub rdb_drop_table_time: HistogramVec,

        #[name = spacetime_rdb_iter_time]
        #[help = "The time spent iterating a table"]
        #[labels(table_id: u32)]
        pub rdb_iter_time: HistogramVec,

        #[name = spacetime_rdb_insert_row_time]
        #[help = "The time spent inserting into a table"]
        #[labels(table_id: u32)]
        pub rdb_insert_row_time: HistogramVec,

        #[name = spacetime_rdb_delete_in_time]
        #[help = "The time spent deleting values in a set from a table"]
        #[labels(table_id: u32)]
        pub rdb_delete_by_rel_time: HistogramVec,

        #[name = spacetime_num_rows_inserted]
        #[help = "The number of rows inserted into a table"]
        #[labels(txn_type: TransactionType, database_id: u64, reducer_id: u64, table_id: u32)]
        pub rdb_num_rows_inserted: IntCounterVec,

        #[name = spacetime_num_rows_deleted]
        #[help = "The number of rows deleted from a table"]
        #[labels(txn_type: TransactionType, database_id: u64, reducer_id: u64, table_id: u32)]
        pub rdb_num_rows_deleted: IntCounterVec,

        #[name = spacetime_num_rows_fetched]
        #[help = "The number of rows fetched from a table"]
        #[labels(txn_type: TransactionType, database_id: u64, reducer_id: u64, table_id: u32)]
        pub rdb_num_rows_fetched: IntCounterVec,
    }
);

pub static DB_METRICS: Lazy<DbMetrics> = Lazy::new(DbMetrics::new);
