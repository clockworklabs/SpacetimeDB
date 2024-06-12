use crate::execution_context::WorkloadType;
use crate::hash::Hash;
use once_cell::sync::Lazy;
use prometheus::{HistogramVec, IntCounterVec, IntGaugeVec};
use spacetimedb_lib::{Address, Identity};
use spacetimedb_metrics::metrics_group;

metrics_group!(
    pub struct WorkerMetrics {
        #[name = spacetime_worker_connected_clients]
        #[help = "Number of clients connected to the worker."]
        #[labels(database_address: Address)]
        pub connected_clients: IntGaugeVec,

        #[name = spacetime_websocket_requests]
        #[help = "Number of websocket request messages"]
        #[labels(instance_id: u64, protocol: str)]
        pub websocket_requests: IntCounterVec,

        #[name = spacetime_websocket_request_msg_size]
        #[help = "The size of messages received on connected sessions"]
        #[labels(instance_id: u64, protocol: str)]
        pub websocket_request_msg_size: HistogramVec,

        #[name = spacetime_websocket_sent]
        #[help = "Number of websocket messages sent to client"]
        #[labels(identity: Identity)]
        pub websocket_sent: IntCounterVec,

        #[name = spacetime_websocket_sent_msg_size]
        #[help = "The size of messages sent to connected sessions"]
        #[labels(identity: Identity)]
        pub websocket_sent_msg_size: HistogramVec,

        #[name = spacetime_worker_instance_operation_queue_length]
        #[help = "Length of the wait queue for access to a module instance."]
        #[labels(database_address: Address)]
        pub instance_queue_length: IntGaugeVec,

        #[name = spacetime_worker_instance_operation_queue_length_histogram]
        #[help = "Length of the wait queue for access to a module instance."]
        #[labels(database_address: Address)]
        #[buckets(0, 10, 25, 50, 75, 100, 150, 200, 250, 300, 350, 400, 450, 500, 1000)]
        pub instance_queue_length_histogram: HistogramVec,

        #[name = spacetime_reducer_wait_time_sec]
        #[help = "The amount of time (in seconds) a reducer spends in the queue waiting to run"]
        #[labels(db: Address, reducer: str)]
        #[buckets(
            1e-6, 5e-6, 1e-5, 5e-5, 1e-4, 5e-4, 1e-3, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0
        )]
        pub reducer_wait_time: HistogramVec,

        #[name = spacetime_worker_wasm_instance_errors_cumulative]
        #[help = "The number of fatal WASM instance errors, such as reducer panics."]
        #[labels(identity: Identity, module_hash: Hash, database_address: Address, reducer_symbol: str)]
        pub wasm_instance_errors: IntCounterVec,

        #[name = spacetime_active_queries]
        #[help = "The number of active subscription queries"]
        #[labels(database_address: Address)]
        pub subscription_queries: IntGaugeVec,

        #[name = spacetime_request_round_trip_time]
        #[help = "The total time it takes for request to complete"]
        #[labels(txn_type: WorkloadType, database_address: Address, reducer_symbol: str)]
        pub request_round_trip: HistogramVec,

        #[name = spacetime_reducer_plus_query_duration_sec]
        #[help = "The time spent executing a reducer (in seconds), plus the time spent evaluating its subscription queries"]
        #[labels(db: Address, reducer: str)]
        pub reducer_plus_query_duration: HistogramVec,
    }
);

pub static WORKER_METRICS: Lazy<WorkerMetrics> = Lazy::new(WorkerMetrics::new);
