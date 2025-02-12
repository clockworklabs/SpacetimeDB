use once_cell::sync::Lazy;
use prometheus::IntGaugeVec;
use spacetimedb_lib::Identity;
use spacetimedb_metrics::metrics_group;

use crate::worker_metrics::WORKER_METRICS;

metrics_group!(
    #[non_exhaustive]
    pub struct DbDataSize {
        #[name = spacetime_total_rows]
        #[help = "The number of rows in a database"]
        #[labels(db: Identity)]
        pub total_rows: IntGaugeVec,

        #[name = spacetime_total_bytes_used_by_rows]
        #[help = "The number of rows in a database"]
        #[labels(db: Identity)]
        pub total_bytes_used_by_rows: IntGaugeVec,

        #[name = spacetime_total_rows_in_indexes]
        #[help = "The number of rows stored in indexes all tables"]
        #[labels(db: Identity)]
        pub total_rows_in_indexes: IntGaugeVec,

        #[name = spacetime_total_bytes_used_by_index_keys]
        #[help = "The number of bytes used by keys stored in indexes in all tables"]
        #[labels(db: Identity)]
        pub total_bytes_used_by_index_keys: IntGaugeVec,

        #[name = spacetime_data_size_table_num_rows]
        #[help = "The number of rows in a table"]
        #[labels(db: Identity, table_id: u32, table_name: str)]
        pub data_size_table_num_rows: IntGaugeVec,

        #[name = spacetime_data_size_bytes_used_by_rows]
        #[help = "The number of bytes used by rows in pages in a table"]
        #[labels(db: Identity, table_id: u32, table_name: str)]
        pub data_size_table_bytes_used_by_rows: IntGaugeVec,

        #[name = spacetime_data_size_table_num_rows_in_indexes]
        #[help = "The number of rows stored in indexes in a table"]
        // TODO: Consider partitioning by index ID or index name.
        #[labels(db: Identity, table_id: u32, table_name: str)]
        pub data_size_table_num_rows_in_indexes: IntGaugeVec,

        #[name = spacetime_data_size_table_bytes_used_by_index_keys]
        #[help = "The number of bytes used by keys stored in indexes in a table"]
        #[labels(db: Identity, table_id: u32, table_name: str)]
        pub data_size_table_bytes_used_by_index_keys: IntGaugeVec,

        #[name = spacetime_data_size_blob_store_num_blobs]
        #[help = "The number of large blobs stored in a database's blob store"]
        #[labels(db: Identity)]
        pub data_size_blob_store_num_blobs: IntGaugeVec,

        #[name = spacetime_data_size_blob_store_bytes_used_by_blobs]
        #[help = "The number of bytes used by large blobs stored in a database's blob store"]
        #[labels(db: Identity)]
        pub data_size_blob_store_bytes_used_by_blobs: IntGaugeVec,
    }
);

pub static DATA_SIZE_METRICS: Lazy<DbDataSize> = Lazy::new(DbDataSize::new);

// Set the value of all the db-level gauges to zero.
// This is useful if a database is being deleted.
pub fn zero_database_gauges(db: &Identity) {
    DATA_SIZE_METRICS.total_rows.with_label_values(db).set(0);
    DATA_SIZE_METRICS.total_bytes_used_by_rows.with_label_values(db).set(0);
    DATA_SIZE_METRICS.total_rows_in_indexes.with_label_values(db).set(0);
    DATA_SIZE_METRICS
        .total_bytes_used_by_index_keys
        .with_label_values(db)
        .set(0);
    DATA_SIZE_METRICS
        .data_size_blob_store_num_blobs
        .with_label_values(db)
        .set(0);
    DATA_SIZE_METRICS
        .data_size_blob_store_bytes_used_by_blobs
        .with_label_values(db)
        .set(0);
    WORKER_METRICS.wasm_memory_bytes.with_label_values(db).set(0);
}
