use once_cell::sync::Lazy;
use prometheus::IntGaugeVec;
use spacetimedb_lib::Address;
use spacetimedb_metrics::metrics_group;

metrics_group!(
    #[non_exhaustive]
    pub struct Metrics {
        #[name = spacetime_num_table_rows]
        #[help = "The number of rows in a table"]
        #[labels(db: Address, table_id: u32, table_name: str)]
        pub rdb_num_table_rows: IntGaugeVec,
    }
);

pub static METRICS: Lazy<Metrics> = Lazy::new(Metrics::new);
