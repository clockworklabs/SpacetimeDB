use once_cell::sync::Lazy;
use prometheus::{Histogram, HistogramOpts, HistogramVec, Registry};

#[non_exhaustive]
pub struct DbMetrics {
    pub registry: Registry,
    pub tdb_insert_time: Histogram,
    pub tdb_delete_time: Histogram,
    pub tdb_seek_time: Histogram,
    pub tdb_scan_time: Histogram,
    pub tdb_commit_time: Histogram,
    pub rdb_create_table_time: HistogramVec,
    pub rdb_drop_table_time: HistogramVec,
    pub rdb_iter_time: HistogramVec,
    pub rdb_insert_row_time: HistogramVec,
    pub rdb_delete_by_rel_time: HistogramVec,
}

pub static DB_METRICS: Lazy<DbMetrics> = Lazy::new(DbMetrics::new);

impl DbMetrics {
    fn new() -> Self {
        DbMetrics {
            registry: Registry::new(),
            tdb_insert_time: Histogram::with_opts(HistogramOpts::new(
                "spacetime_tdb_insert_time",
                "Time time it takes for the transactional store to perform an insert",
            ))
            .unwrap(),
            tdb_delete_time: Histogram::with_opts(HistogramOpts::new(
                "spacetime_tdb_delete_time",
                "Time time it takes for the transactional store to perform a delete",
            ))
            .unwrap(),
            tdb_seek_time: Histogram::with_opts(HistogramOpts::new(
                "spacetime_tdb_seek_time",
                "Time time it takes for the transactional store to perform a seek",
            ))
            .unwrap(),
            tdb_scan_time: Histogram::with_opts(HistogramOpts::new(
                "spacetime_tdb_scan_time",
                "Time time it takes for the transactional store to perform a scan",
            ))
            .unwrap(),
            tdb_commit_time: Histogram::with_opts(HistogramOpts::new(
                "spacetime_tdb_commit_time",
                "Time time it takes for the transactional store to perform a Tx commit",
            ))
            .unwrap(),
            rdb_create_table_time: HistogramVec::new(
                HistogramOpts::new("spacetime_rdb_create_table_time", "The time it takes to create a table"),
                &["table_name"],
            )
            .unwrap(),
            rdb_drop_table_time: HistogramVec::new(
                HistogramOpts::new("spacetime_rdb_drop_table_time", "The time spent dropping a table"),
                &["table_id"],
            )
            .unwrap(),
            rdb_iter_time: HistogramVec::new(
                HistogramOpts::new("spacetime_rdb_iter_time", "The time spent iterating a table"),
                &["table_id"],
            )
            .unwrap(),
            rdb_insert_row_time: HistogramVec::new(
                HistogramOpts::new("spacetime_rdb_insert_row_time", "The time spent inserting into a table"),
                &["table_id"],
            )
            .unwrap(),
            rdb_delete_by_rel_time: HistogramVec::new(
                HistogramOpts::new(
                    "spacetime_rdb_delete_in_time",
                    "The time spent deleting values in a set from a table",
                ),
                &["table_id"],
            )
            .unwrap(),
        }
    }

    pub fn register_custom_metrics(&self) {
        self.registry.register(Box::new(self.tdb_insert_time.clone())).unwrap();
        self.registry.register(Box::new(self.tdb_delete_time.clone())).unwrap();
        self.registry.register(Box::new(self.tdb_seek_time.clone())).unwrap();
        self.registry.register(Box::new(self.tdb_scan_time.clone())).unwrap();
        self.registry.register(Box::new(self.tdb_commit_time.clone())).unwrap();

        self.registry
            .register(Box::new(self.rdb_create_table_time.clone()))
            .unwrap();
        self.registry
            .register(Box::new(self.rdb_drop_table_time.clone()))
            .unwrap();
        self.registry.register(Box::new(self.rdb_iter_time.clone())).unwrap();
        self.registry
            .register(Box::new(self.rdb_insert_row_time.clone()))
            .unwrap();
        self.registry
            .register(Box::new(self.rdb_delete_by_rel_time.clone()))
            .unwrap();
    }
}

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

pub fn register_custom_metrics() {
    DB_METRICS.register_custom_metrics()
}
