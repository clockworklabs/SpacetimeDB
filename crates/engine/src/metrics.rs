use once_cell::sync::Lazy;
use prometheus::{GaugeVec, HistogramVec, IntCounter, IntCounterVec, IntGaugeVec};
use spacetimedb_datastore::{
    db_metrics::DB_METRICS, execution_context::WorkloadType, locking_tx_datastore::datastore::MetricsRecorder,
};
use spacetimedb_lib::{metrics::ExecutionMetrics, Identity};
use spacetimedb_metrics::metrics_group;

metrics_group!(
    pub struct EngineMetrics {
        #[name = spacetime_num_bytes_sent_to_clients_total]
        #[help = "The cumulative number of bytes sent to clients"]
        #[labels(txn_type: WorkloadType, db: Identity)]
        pub bytes_sent_to_clients: IntCounterVec,

        #[name = spacetime_replay_total_time_seconds]
        #[help = "Total time spent replaying a database upon restart, including snapshot read, snapshot restore and commitlog replay"]
        #[labels(db: Identity)]
        pub replay_total_time_seconds: GaugeVec,

        #[name = spacetime_replay_snapshot_read_time_seconds]
        #[help = "Time spent reading a snapshot from disk before restoring the snapshot upon restart"]
        #[labels(db: Identity)]
        pub replay_snapshot_read_time_seconds: GaugeVec,

        #[name = spacetime_replay_snapshot_restore_time_seconds]
        #[help = "Time spent restoring a database from a snapshot after reading the snapshot and before commitlog replay upon restart"]
        #[labels(db: Identity)]
        pub replay_snapshot_restore_time_seconds: GaugeVec,

        #[name = spacetime_replay_commitlog_time_seconds]
        #[help = "Time spent replaying the commitlog after restoring from a snapshot upon restart"]
        #[labels(db: Identity)]
        pub replay_commitlog_time_seconds: GaugeVec,

        #[name = spacetime_replay_commitlog_num_commits]
        #[help = "Number of commits replayed after restoring from a snapshot upon restart"]
        #[labels(db: Identity)]
        pub replay_commitlog_num_commits: IntGaugeVec,

        // Snapshot creation should take in the order of milliseconds,
        // but log data suggests that there are outliers.
        // So let's track a wide range of buckets to get a better picture.
        //
        // We also track the timing without `asyncify` scheduling overhead
        // (`snapshot_creation_time_inner`), and the snapshot compression
        // timing with / without scheduling overhead (`snapshot_compression_time_total`
        // and `snapshot_compression_time_inner`, respectively).
        //
        // Compression may have contributed to observed outliers, but is no
        // longer included in the snapshot creation timing.
        #[name = spacetime_snapshot_creation_time_total_sec]
        #[help = "The time (in seconds) it took to take and store a database snapshot, including scheduling overhead"]
        #[labels(db: Identity)]
        #[buckets(0.0005, 0.001, 0.005, 0.01, 0.1, 1.0, 5.0, 10.0)]
        pub snapshot_creation_time_total: HistogramVec,

        #[name = spacetime_snapshot_creation_time_inner_sec]
        #[help = "The time (in seconds) it took to take and store a database snapshot, excluding scheduling overhead"]
        #[labels(db: Identity)]
        #[buckets(0.0005, 0.001, 0.005, 0.01, 0.1, 1.0, 5.0, 10.0)]
        pub snapshot_creation_time_inner: HistogramVec,

        #[name = spacetime_snapshot_creation_time_fsync_sec]
        #[help = "The time (in seconds) it took to fsync a database snapshot, excluding scheduling overhead"]
        #[labels(db: Identity)]
        #[buckets(0.0005, 0.001, 0.005, 0.01, 0.1, 1.0, 5.0, 10.0)]
        pub snapshot_creation_time_fsync: HistogramVec,

        #[name = spacetime_snapshot_compression_time_total_sec]
        #[help = "The time (in seconds) it took to do a compression pass on the snapshot repository, including scheduling overhead"]
        #[labels(db: Identity)]
        #[buckets(0.001, 0.01, 0.1, 1.0, 5.0, 10.0)]
        pub snapshot_compression_time_total: HistogramVec,

        #[name = spacetime_snapshot_compression_time_inner_sec]
        #[help = "The time (in seconds) it took to do a compression pass on the snapshot repository, excluding scheduling overhead"]
        #[labels(db: Identity)]
        #[buckets(0.001, 0.01, 0.1, 1.0, 5.0, 10.0)]
        pub snapshot_compression_time_inner: HistogramVec,

        #[name = spacetime_snapshot_compression_time_per_snapshot_sec]
        #[help = "The time (in seconds) it took to compress a single snapshot"]
        #[labels(db: Identity)]
        #[buckets(0.001, 0.01, 0.1, 1.0, 5.0, 10.0)]
        pub snapshot_compression_time_single: HistogramVec,

        #[name = spacetime_snapshot_compression_skipped]
        #[help = "The number of snapshots skipped in a single compression pass because they were already compressed"]
        #[labels(db: Identity)]
        pub snapshot_compression_skipped: IntGaugeVec,

        #[name = spacetime_snapshot_compression_compressed]
        #[help = "The number of snapshots compressed in a single compression pass"]
        #[labels(db: Identity)]
        pub snapshot_compression_compressed: IntGaugeVec,

        #[name = spacetime_snapshot_compression_objects_compressed]
        #[help = "The number of snapshot objects compressed in a single compression pass"]
        #[labels(db: Identity)]
        pub snapshot_compression_objects_compressed: IntGaugeVec,

        #[name = spacetime_snapshot_compression_objects_hardlinked]
        #[help = "The number of snapshot objects hardlinked in a single compression pass"]
        #[labels(db: Identity)]
        pub snapshot_compression_objects_hardlinked: IntGaugeVec,

        #[name = spacetime_durability_blocking_send_duration_sec]
        #[help = "Latency of blocking sends in request_durability (seconds); _count gives the number of times the channel was full"]
        #[labels(database_identity: Identity)]
        #[buckets(0.001, 0.01, 0.1, 1.0, 10.0)]
        pub durability_blocking_send_duration: HistogramVec,
    }
);

pub static ENGINE_METRICS: Lazy<EngineMetrics> = Lazy::new(EngineMetrics::new);

#[derive(Debug)]
pub struct ExecutionCounters {
    rdb_num_index_seeks: IntCounter,
    rdb_num_rows_scanned: IntCounter,
    rdb_num_bytes_scanned: IntCounter,
    rdb_num_bytes_written: IntCounter,
    bytes_sent_to_clients: IntCounter,
    delta_queries_matched: IntCounter,
    delta_queries_evaluated: IntCounter,
    duplicate_rows_evaluated: IntCounter,
    duplicate_rows_sent: IntCounter,
}

impl ExecutionCounters {
    pub fn new(workload: &WorkloadType, db: &Identity) -> Self {
        Self {
            rdb_num_index_seeks: DB_METRICS.rdb_num_index_seeks.with_label_values(workload, db),
            rdb_num_rows_scanned: DB_METRICS.rdb_num_rows_scanned.with_label_values(workload, db),
            rdb_num_bytes_scanned: DB_METRICS.rdb_num_bytes_scanned.with_label_values(workload, db),
            rdb_num_bytes_written: DB_METRICS.rdb_num_bytes_written.with_label_values(workload, db),
            bytes_sent_to_clients: ENGINE_METRICS.bytes_sent_to_clients.with_label_values(workload, db),
            delta_queries_matched: DB_METRICS.delta_queries_matched.with_label_values(db),
            delta_queries_evaluated: DB_METRICS.delta_queries_evaluated.with_label_values(db),
            duplicate_rows_evaluated: DB_METRICS.duplicate_rows_evaluated.with_label_values(db),
            duplicate_rows_sent: DB_METRICS.duplicate_rows_sent.with_label_values(db),
        }
    }

    pub fn record(&self, metrics: &ExecutionMetrics) {
        if metrics.index_seeks > 0 {
            self.rdb_num_index_seeks.inc_by(metrics.index_seeks as u64);
        }
        if metrics.rows_scanned > 0 {
            self.rdb_num_rows_scanned.inc_by(metrics.rows_scanned as u64);
        }
        if metrics.bytes_scanned > 0 {
            self.rdb_num_bytes_scanned.inc_by(metrics.bytes_scanned as u64);
        }
        if metrics.bytes_written > 0 {
            self.rdb_num_bytes_written.inc_by(metrics.bytes_written as u64);
        }
        if metrics.bytes_sent_to_clients > 0 {
            self.bytes_sent_to_clients.inc_by(metrics.bytes_sent_to_clients as u64);
        }
        if metrics.delta_queries_matched > 0 {
            self.delta_queries_matched.inc_by(metrics.delta_queries_matched);
        }
        if metrics.delta_queries_evaluated > 0 {
            self.delta_queries_evaluated.inc_by(metrics.delta_queries_evaluated);
        }
        if metrics.duplicate_rows_evaluated > 0 {
            self.duplicate_rows_evaluated.inc_by(metrics.duplicate_rows_evaluated);
        }
        if metrics.duplicate_rows_sent > 0 {
            self.duplicate_rows_sent.inc_by(metrics.duplicate_rows_sent);
        }
    }
}

impl MetricsRecorder for ExecutionCounters {
    fn record(&self, metrics: &ExecutionMetrics) {
        self.record(metrics);
    }
}
