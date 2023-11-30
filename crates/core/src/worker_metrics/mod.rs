use std::{collections::HashMap, sync::Mutex};

use crate::hash::Hash;
use crate::util::typed_prometheus::metrics_group;
use once_cell::sync::Lazy;
use prometheus::{Gauge, GaugeVec, HistogramVec, IntCounterVec, IntGaugeVec};
use spacetimedb_lib::{Address, Identity};

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

        #[name = spacetime_worker_process_cpu_usage]
        #[help = "CPU usage of the worker process."]
        pub process_cpu_usage: Gauge,

        #[name = spacetime_worker_transactions]
        #[help = "Number of reducer calls."]
        #[labels(database_address: Address, reducer_symbol: str)]
        pub reducer_count: IntCounterVec,

        #[name = spacetime_worker_module_tx_compute_time]
        #[help = "The time it takes to compute and commit after reducer execution."]
        #[labels(database_address: Address, reducer_symbol: str)]
        pub reducer_compute_time: HistogramVec,

        #[name = spacetime_worker_tx_size]
        #[help = "The size of committed bytes in the message log after reducer execution."]
        #[labels(database_address: Address, reducer_symbol: str)]
        pub reducer_write_size: HistogramVec,

        #[name = spacetime_worker_identity_energy_budget]
        #[help = "Node-level energy budget, per identity"]
        #[labels(identity: Identity, node: u64)]
        pub node_identity_energy_budget_gauge: GaugeVec,

        #[name = spacetime_instance_env_insert]
        #[help = "Time spent by reducers inserting rows (InstanceEnv::insert)"]
        #[labels(database_address: Address, table_id: u32)]
        pub instance_env_insert: HistogramVec,

        #[name = spacetime_instance_env_delete_eq]
        #[help = "Time spent by reducers deleting rows by eq (InstanceEnv::delete_eq)"]
        #[labels(database_address: Address, table_id: u32)]
        pub instance_env_delete_eq: HistogramVec,

        #[name = spacetime_worker_instance_operation_queue_length]
        #[help = "Length of the wait queue for access to a module instance."]
        #[labels(database_address: Address)]
        pub instance_queue_length: IntGaugeVec,

        #[name = spacetime_worker_instance_operation_queue_length_max]
        #[help = "Max length of the wait queue for access to a module instance."]
        #[labels(database_address: Address)]
        pub instance_queue_length_max: IntGaugeVec,

        #[name = spacetime_worker_instance_operation_queue_length_histogram]
        #[help = "Length of the wait queue for access to a module instance."]
        #[labels(database_address: Address)]
        #[buckets(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 15, 25, 50)]
        pub instance_queue_length_histogram: HistogramVec,

        #[name = spacetime_scheduled_reducer_delay_sec]
        #[help = "The amount of time (in seconds) a reducer has been delayed past its scheduled execution time"]
        #[labels(db: Address, reducer: str)]
        #[buckets(
            1e-6, 5e-6, 1e-5, 5e-5, 1e-4, 5e-4, 1e-3, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0
        )]
        pub scheduled_reducer_delay_sec: HistogramVec,

        #[name = spacetime_scheduled_reducer_delay_sec_max]
        #[help = "The maximum duration (in seconds) a reducer has been delayed"]
        #[labels(db: Address, reducer: str)]
        pub scheduled_reducer_delay_sec_max: GaugeVec,

        #[name = spacetime_worker_wasm_instance_errors_cumulative]
        #[help = "The number of fatal WASM instance errors, such as reducer panics."]
        #[labels(identity: Identity, module_hash: Hash, database_address: Address, reducer_symbol: str)]
        pub wasm_instance_errors: IntCounterVec,
    }
);

pub static MAX_QUEUE_LEN: Lazy<Mutex<HashMap<Address, i64>>> = Lazy::new(|| Mutex::new(HashMap::new()));
pub static MAX_REDUCER_DELAY: Lazy<Mutex<HashMap<u64, f64>>> = Lazy::new(|| Mutex::new(HashMap::new()));
pub static WORKER_METRICS: Lazy<WorkerMetrics> = Lazy::new(WorkerMetrics::new);

pub fn reset_counters() {
    MAX_QUEUE_LEN.lock().unwrap().clear();
    MAX_REDUCER_DELAY.lock().unwrap().clear();
}
