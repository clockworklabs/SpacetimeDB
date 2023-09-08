use crate::worker_metrics::{metrics_delegator, metrics_group};
use once_cell::sync::Lazy;
use prometheus::{Histogram, HistogramVec, Registry};

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
        #[labels(table_name)]
        pub rdb_create_table_time: HistogramVec,

        #[name = spacetime_rdb_drop_table_time]
        #[help = "The time spent dropping a table"]
        #[labels(table_id)]
        pub rdb_drop_table_time: HistogramVec,

        #[name = spacetime_rdb_iter_time]
        #[help = "The time spent iterating a table"]
        #[labels(table_id)]
        pub rdb_iter_time: HistogramVec,

        #[name = spacetime_rdb_insert_row_time]
        #[help = "The time spent inserting into a table"]
        #[labels(table_id)]
        pub rdb_insert_row_time: HistogramVec,

        #[name = spacetime_rdb_delete_in_time]
        #[help = "The time spent deleting values in a set from a table"]
        #[labels(table_id)]
        pub rdb_delete_by_rel_time: HistogramVec,
    }
);

pub static DB_METRICS: Lazy<DbMetrics> = Lazy::new(DbMetrics::new);

use DB_METRICS as METRICS;
metrics_delegator!(REGISTRY, registry: Registry);
metrics_delegator!(TDB_INSERT_TIME, tdb_insert_time: Histogram);
metrics_delegator!(TDB_DELETE_TIME, tdb_delete_time: Histogram);
metrics_delegator!(TDB_SEEK_TIME, tdb_seek_time: Histogram);
metrics_delegator!(TDB_SCAN_TIME, tdb_scan_time: Histogram);
metrics_delegator!(TDB_COMMIT_TIME, tdb_commit_time: Histogram);
metrics_delegator!(RDB_CREATE_TABLE_TIME, rdb_create_table_time: HistogramVec);
metrics_delegator!(RDB_DROP_TABLE_TIME, rdb_drop_table_time: HistogramVec);
metrics_delegator!(RDB_ITER_TIME, rdb_iter_time: HistogramVec);
metrics_delegator!(RDB_INSERT_TIME, rdb_insert_row_time: HistogramVec);
metrics_delegator!(RDB_DELETE_BY_REL_TIME, rdb_delete_by_rel_time: HistogramVec);
