use crate::util::typed_prometheus::metrics_group;
use once_cell::sync::Lazy;
use prometheus::{Gauge, GaugeVec, HistogramVec, IntCounterVec, IntGauge, IntGaugeVec};
use spacetimedb_lib::{Address, Identity};
use spacetimedb_sats::hash::Hash;

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
        #[labels(identity: Identity, module_hash: Hash, database_address: Address, reducer_symbol: str)]
        pub instance_queue_length: IntGaugeVec,

        #[name = spacetime_system_disk_space_total_bytes]
        #[help = "A node's total disk space (in bytes)"]
        pub system_disk_space_total: IntGauge,

        #[name = spacetime_system_disk_space_free_bytes]
        #[help = "A node's free (unused) disk space (in bytes)"]
        pub system_disk_space_free: IntGauge,

        #[name = spacetime_system_memory_total_bytes]
        #[help = "A node's total available memory (in bytes)"]
        pub system_memory_total: IntGauge,

        #[name = spacetime_system_memory_free_bytes]
        #[help = "A node's current available (free) memory (in bytes)"]
        pub system_memory_free: IntGauge,
    }
);

pub static WORKER_METRICS: Lazy<WorkerMetrics> = Lazy::new(WorkerMetrics::new);
