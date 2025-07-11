use once_cell::sync::Lazy;
use prometheus::IntGaugeVec;
use spacetimedb_lib::Identity;
use spacetimedb_metrics::metrics_group;

metrics_group!(
    #[non_exhaustive]
    pub struct DbDataSize {
        #[name = spacetime_data_size_table_num_rows]
        #[help = "The number of rows in a table"]
        #[labels(db: Identity, table_name: str)]
        pub data_size_table_num_rows: IntGaugeVec,

        #[name = spacetime_data_size_bytes_used_by_rows]
        #[help = "The number of bytes used by rows in pages in a table"]
        #[labels(db: Identity, table_name: str)]
        pub data_size_table_bytes_used_by_rows: IntGaugeVec,

        #[name = spacetime_data_size_table_num_rows_in_indexes]
        #[help = "The number of rows stored in indexes in a table"]
        // TODO: Consider partitioning by index ID or index name.
        #[labels(db: Identity, table_name: str)]
        pub data_size_table_num_rows_in_indexes: IntGaugeVec,

        #[name = spacetime_data_size_table_bytes_used_by_index_keys]
        #[help = "The number of bytes used by keys stored in indexes in a table"]
        #[labels(db: Identity, table_name: str)]
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
