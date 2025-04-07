use std::time::Duration;

use crate::execution_context::WorkloadType;
use crate::hash::Hash;
use once_cell::sync::Lazy;
use prometheus::{HistogramVec, IntCounterVec, IntGaugeVec};
use spacetimedb_lib::{ConnectionId, Identity};
use spacetimedb_metrics::metrics_group;

metrics_group!(
    pub struct WorkerMetrics {
        #[name = spacetime_worker_connected_clients]
        #[help = "Number of clients connected to the worker."]
        #[labels(database_identity: Identity)]
        pub connected_clients: IntGaugeVec,

        #[name = spacetime_websocket_requests_total]
        #[help = "The cumulative number of websocket request messages"]
        #[labels(replica_id: u64, protocol: str)]
        pub websocket_requests: IntCounterVec,

        #[name = spacetime_websocket_request_msg_size]
        #[help = "The size of messages received on connected sessions"]
        #[labels(replica_id: u64, protocol: str)]
        pub websocket_request_msg_size: HistogramVec,

        #[name = jemalloc_active_bytes]
        #[help = "Number of bytes in jemallocs heap"]
        #[labels(node_id: str)]
        pub jemalloc_active_bytes: IntGaugeVec,

        #[name = jemalloc_allocated_bytes]
        #[help = "Number of bytes in use by the application"]
        #[labels(node_id: str)]
        pub jemalloc_allocated_bytes: IntGaugeVec,

        #[name = jemalloc_resident_bytes]
        #[help = "Total memory used by jemalloc"]
        #[labels(node_id: str)]
        pub jemalloc_resident_bytes: IntGaugeVec,

        #[name = tokio_num_workers]
        #[help = "Number of core tokio workers"]
        #[labels(node_id: str)]
        pub tokio_num_workers: IntGaugeVec,

        #[name = tokio_num_blocking_threads]
        #[help = "Number of extra tokio threads for blocking tasks"]
        #[labels(node_id: str)]
        pub tokio_num_blocking_threads: IntGaugeVec,

        #[name = tokio_num_idle_blocking_threads]
        #[help = "Number of tokio blocking threads that are idle"]
        #[labels(node_id: str)]
        pub tokio_num_idle_blocking_threads: IntGaugeVec,

        #[name = tokio_num_alive_tasks]
        #[help = "Number of tokio tasks that are still alive"]
        #[labels(node_id: str)]
        pub tokio_num_alive_tasks: IntGaugeVec,

        #[name = tokio_global_queue_depth]
        #[help = "Number of tasks in tokios global queue"]
        #[labels(node_id: str)]
        pub tokio_global_queue_depth: IntGaugeVec,

        #[name = tokio_blocking_queue_depth]
        #[help = "Number of tasks in tokios blocking task queue"]
        #[labels(node_id: str)]
        pub tokio_blocking_queue_depth: IntGaugeVec,

        #[name = tokio_spawned_tasks_count]
        #[help = "Number of tokio tasks spawned"]
        #[labels(node_id: str)]
        pub tokio_spawned_tasks_count: IntCounterVec,

        #[name = tokio_remote_schedule_count]
        #[help = "Number of tasks spawned from outside the tokio runtime"]
        #[labels(node_id: str)]
        pub tokio_remote_schedule_count: IntCounterVec,

        #[name = spacetime_websocket_sent_msg_size_bytes]
        #[help = "The size of messages sent to connected sessions"]
        #[labels(db: Identity, workload: WorkloadType)]
        // Prometheus histograms have default buckets,
        // which broadly speaking,
        // are tailored to measure the response time of a network service.
        //
        // Therefore we define specific buckets for this metric,
        // since it has a different unit and a different distribution.
        //
        // In particular incremental update payloads could be smaller than 1KB,
        // whereas initial subscription payloads could exceed 10MB.
        #[buckets(100, 500, 1e3, 10e3, 100e3, 500e3, 1e6, 5e6, 10e6, 25e6, 50e6, 75e6, 100e6, 500e6)]
        pub websocket_sent_msg_size: HistogramVec,

        #[name = spacetime_websocket_sent_num_rows]
        #[help = "The number of rows sent to connected sessions"]
        #[labels(db: Identity, workload: WorkloadType)]
        // Prometheus histograms have default buckets,
        // which broadly speaking,
        // are tailored to measure the response time of a network service.
        //
        // Therefore we define specific buckets for this metric,
        // since it has a different unit and a different distribution.
        //
        // In particular incremental updates could have fewer than 10 rows,
        // whereas initial subscriptions could exceed 100K rows.
        #[buckets(5, 10, 50, 100, 500, 1e3, 5e3, 10e3, 50e3, 100e3, 250e3, 500e3, 750e3, 1e6, 5e6)]
        pub websocket_sent_num_rows: HistogramVec,

        #[name = spacetime_worker_instance_operation_queue_length]
        #[help = "Length of the wait queue for access to a module instance."]
        #[labels(database_identity: Identity)]
        pub instance_queue_length: IntGaugeVec,

        #[name = spacetime_worker_instance_operation_queue_length_histogram]
        #[help = "Length of the wait queue for access to a module instance."]
        #[labels(database_identity: Identity)]
        // Prometheus histograms have default buckets,
        // which broadly speaking,
        // are tailored to measure the response time of a network service.
        // Hence we need to define specific buckets for queue length.
        #[buckets(0, 1, 2, 5, 10, 25, 50, 75, 100, 200, 300, 400, 500, 1000)]
        pub instance_queue_length_histogram: HistogramVec,

        #[name = spacetime_reducer_wait_time_sec]
        #[help = "The amount of time (in seconds) a reducer spends in the queue waiting to run"]
        #[labels(db: Identity, reducer: str)]
        // Prometheus histograms have default buckets,
        // which broadly speaking,
        // are tailored to measure the response time of a network service.
        //
        // However we expect a different value distribution for this metric.
        // In particular the smallest bucket value is 5ms by default.
        // But we expect many wait times to be on the order of microseconds.
        #[buckets(100e-6, 500e-6, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1, 5, 10)]
        pub reducer_wait_time: HistogramVec,

        #[name = spacetime_worker_wasm_instance_errors_total]
        #[help = "The number of fatal WASM instance errors, such as reducer panics."]
        #[labels(caller_identity: Identity, module_hash: Hash, caller_connection_id: ConnectionId, reducer_symbol: str)]
        pub wasm_instance_errors: IntCounterVec,

        #[name = spacetime_worker_wasm_memory_bytes]
        #[help = "The number of bytes of linear memory allocated by the database's WASM module instance"]
        #[labels(database_identity: Identity)]
        pub wasm_memory_bytes: IntGaugeVec,

        #[name = spacetime_active_queries]
        #[help = "The number of active subscription queries"]
        #[labels(database_identity: Identity)]
        pub subscription_queries: IntGaugeVec,

        #[name = spacetime_request_round_trip_time]
        #[help = "The total time it takes for request to complete"]
        #[labels(txn_type: WorkloadType, database_identity: Identity, reducer_symbol: str)]
        pub request_round_trip: HistogramVec,

        #[name = spacetime_reducer_plus_query_duration_sec]
        #[help = "The time spent executing a reducer (in seconds), plus the time spent evaluating its subscription queries"]
        #[labels(db: Identity, reducer: str)]
        pub reducer_plus_query_duration: HistogramVec,

        #[name = spacetime_num_bytes_sent_to_clients_total]
        #[help = "The cumulative number of bytes sent to clients"]
        #[labels(txn_type: WorkloadType, db: Identity)]
        pub bytes_sent_to_clients: IntCounterVec,
    }
);

pub static WORKER_METRICS: Lazy<WorkerMetrics> = Lazy::new(WorkerMetrics::new);

#[cfg(not(target_env = "msvc"))]
use tikv_jemalloc_ctl::{epoch, stats};

use std::sync::Once;
use tokio::{spawn, time::sleep};
static SPAWN_JEMALLOC_GUARD: Once = Once::new();
pub fn spawn_jemalloc_stats(node_id: String) {
    #[cfg(not(target_env = "msvc"))]
    SPAWN_JEMALLOC_GUARD.call_once(|| {
        spawn(async move {
            let e = epoch::mib().unwrap();
            loop {
                e.advance().unwrap();
                let allocated = stats::allocated::read().unwrap();
                WORKER_METRICS
                    .jemalloc_allocated_bytes
                    .with_label_values(&node_id)
                    .set(allocated as i64);
                let resident = stats::resident::read().unwrap();
                WORKER_METRICS
                    .jemalloc_resident_bytes
                    .with_label_values(&node_id)
                    .set(resident as i64);
                let active = stats::active::read().unwrap();
                WORKER_METRICS
                    .jemalloc_active_bytes
                    .with_label_values(&node_id)
                    .set(active as i64);

                sleep(Duration::from_secs(10)).await;
            }
        });
    });
}

// How frequently to update the tokio stats.
const TOKIO_STATS_INTERVAL: Duration = Duration::from_secs(10);
static SPAWN_TOKIO_STATS_GUARD: Once = Once::new();
pub fn spawn_tokio_stats(node_id: String) {
    SPAWN_TOKIO_STATS_GUARD.call_once(|| {
        spawn(async move {
            // Set up our metric handles, so we don't keep calling `with_label_values`.
            let num_worker_metric = WORKER_METRICS.tokio_num_workers.with_label_values(&node_id);
            let num_blocking_threads_metric = WORKER_METRICS.tokio_num_blocking_threads.with_label_values(&node_id);
            let num_alive_tasks_metric = WORKER_METRICS.tokio_num_alive_tasks.with_label_values(&node_id);
            let global_queue_depth_metric = WORKER_METRICS.tokio_global_queue_depth.with_label_values(&node_id);
            let num_idle_blocking_threads_metric = WORKER_METRICS
                .tokio_num_idle_blocking_threads
                .with_label_values(&node_id);
            let blocking_queue_depth_metric = WORKER_METRICS.tokio_blocking_queue_depth.with_label_values(&node_id);
            let spawned_tasks_count_metric = WORKER_METRICS.tokio_spawned_tasks_count.with_label_values(&node_id);
            let remote_schedule_count_metric = WORKER_METRICS.tokio_remote_schedule_count.with_label_values(&node_id);
            loop {
                let metrics = tokio::runtime::Handle::current().metrics();

                num_worker_metric.set(metrics.num_workers() as i64);
                num_alive_tasks_metric.set(metrics.num_alive_tasks() as i64);
                global_queue_depth_metric.set(metrics.global_queue_depth() as i64);
                #[cfg(tokio_unstable)]
                {
                    num_blocking_threads_metric.set(metrics.num_blocking_threads() as i64);
                    num_idle_blocking_threads_metric.set(metrics.num_idle_blocking_threads() as i64);
                    blocking_queue_depth_metric.set(metrics.blocking_queue_depth() as i64);
                }

                // The spawned tasks count and remote schedule count are cumulative,
                // so we need to increment them by the difference from the last value.
                #[cfg(all(target_has_atomic = "64", tokio_unstable))]
                {
                    {
                        let current_count = metrics.spawned_tasks_count();
                        let previous_value = spawned_tasks_count_metric.get();
                        // The tokio metric should be monotonically increasing, but we are checking just in case.
                        if let Some(diff) = current_count.checked_sub(previous_value) {
                            spawned_tasks_count_metric.inc_by(diff);
                        }
                    }
                    {
                        let current_count = metrics.remote_schedule_count();
                        let previous_value = remote_schedule_count_metric.get();
                        // The tokio metric should be monotonically increasing, but we are checking just in case.
                        if let Some(diff) = current_count.checked_sub(previous_value) {
                            remote_schedule_count_metric.inc_by(diff);
                        }
                    }
                }
                #[cfg(target_has_atomic = "64")]
                // TODO: Consider adding some of the worker metrics as well, like overflows, steals, etc.
                sleep(TOKIO_STATS_INTERVAL).await;
            }
        });
    });
}
