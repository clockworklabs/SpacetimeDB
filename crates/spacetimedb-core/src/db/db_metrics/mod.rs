use lazy_static::lazy_static;
use prometheus::{Histogram, HistogramOpts, HistogramVec, Registry};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    pub static ref TDB_INSERT_TIME: Histogram = Histogram::with_opts(HistogramOpts::new(
        "spacetime_tdb_insert_time",
        "Time time it takes for the transactional store to perform an insert"
    ))
    .unwrap();
    pub static ref TDB_DELETE_TIME: Histogram = Histogram::with_opts(HistogramOpts::new(
        "spacetime_tdb_delete_time",
        "Time time it takes for the transactional store to perform a delete"
    ))
    .unwrap();
    pub static ref TDB_SEEK_TIME: Histogram = Histogram::with_opts(HistogramOpts::new(
        "spacetime_tdb_seek_time",
        "Time time it takes for the transactional store to perform a seek"
    ))
    .unwrap();
    pub static ref TDB_SCAN_TIME: Histogram = Histogram::with_opts(HistogramOpts::new(
        "spacetime_tdb_scan_time",
        "Time time it takes for the transactional store to perform a scan"
    ))
    .unwrap();
    pub static ref TDB_COMMIT_TIME: Histogram = Histogram::with_opts(HistogramOpts::new(
        "spacetime_tdb_commit_time",
        "Time time it takes for the transactional store to perform a Tx commit"
    ))
    .unwrap();
    pub static ref RDB_CREATE_TABLE_TIME: HistogramVec = HistogramVec::new(
        HistogramOpts::new("spacetime_rdb_create_table_time", "The time it takes to create a table"),
        &["table_name"]
    )
    .unwrap();
    pub static ref RDB_DROP_TABLE_TIME: HistogramVec = HistogramVec::new(
        HistogramOpts::new("spacetime_rdb_drop_table_time", "The time spent dropping a table"),
        &["table_id"]
    )
    .unwrap();
    pub static ref RDB_SCAN_TIME: HistogramVec = HistogramVec::new(
        HistogramOpts::new("spacetime_rdb_scan_time", "The time spent scanning a table"),
        &["table_id"]
    )
    .unwrap();
    pub static ref RDB_SCAN_RAW_TIME: HistogramVec = HistogramVec::new(
        HistogramOpts::new("spacetime_rdb_scan_raw_time", "The time spent scanning a table"),
        &["table_id"]
    )
    .unwrap();
    pub static ref RDB_SCAN_PK_TIME: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_rdb_scan_pk_time",
            "The time spent scanning a table using its primary key"
        ),
        &["table_id"]
    )
    .unwrap();
    pub static ref RDB_INSERT_TIME: HistogramVec = HistogramVec::new(
        HistogramOpts::new("spacetime_rdb_insert_time", "The time spent inserting into a table"),
        &["table_id"]
    )
    .unwrap();
    pub static ref RDB_DELETE_PK_TIME: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_rdb_delete_pk_time",
            "The time spent deleting from a table using its primary key",
        ),
        &["table_id"]
    )
    .unwrap();
    pub static ref RDB_DELETE_IN_TIME: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_rdb_delete_in_time",
            "The time spent deleting values in a set from a table",
        ),
        &["table_id"]
    )
    .unwrap();
}

pub fn register_custom_metrics() {
    REGISTRY.register(Box::new(TDB_INSERT_TIME.clone())).unwrap();
    REGISTRY.register(Box::new(TDB_DELETE_TIME.clone())).unwrap();
    REGISTRY.register(Box::new(TDB_SEEK_TIME.clone())).unwrap();
    REGISTRY.register(Box::new(TDB_SCAN_TIME.clone())).unwrap();
    REGISTRY.register(Box::new(TDB_COMMIT_TIME.clone())).unwrap();

    REGISTRY.register(Box::new(RDB_CREATE_TABLE_TIME.clone())).unwrap();
    REGISTRY.register(Box::new(RDB_DROP_TABLE_TIME.clone())).unwrap();
    REGISTRY.register(Box::new(RDB_SCAN_TIME.clone())).unwrap();
    REGISTRY.register(Box::new(RDB_SCAN_RAW_TIME.clone())).unwrap();
    REGISTRY.register(Box::new(RDB_SCAN_PK_TIME.clone())).unwrap();
    REGISTRY.register(Box::new(RDB_INSERT_TIME.clone())).unwrap();
    REGISTRY.register(Box::new(RDB_DELETE_PK_TIME.clone())).unwrap();
    REGISTRY.register(Box::new(RDB_DELETE_IN_TIME.clone())).unwrap();
}
