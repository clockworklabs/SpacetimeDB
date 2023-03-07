use lazy_static::lazy_static;
use prometheus::{Gauge, GaugeVec, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    pub static ref CONNECTED_CLIENTS: IntGauge = IntGauge::new(
        "spacetime_worker_connected_clients",
        "Number of clients connected to the worker."
    )
    .unwrap();
    pub static ref WEBSOCKET_REQUESTS: IntCounterVec = IntCounterVec::new(
        Opts::new("spacetime_websocket_requests", "Number of websocket request messages"),
        &["instance_id", "protocol"]
    )
    .unwrap();
    pub static ref WEBSOCKET_REQUEST_MSG_SIZE: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_websocket_request_msg_size",
            "The size of messages received on connected sessions"
        ),
        &["instance_id", "protocol"]
    )
    .unwrap();
    pub static ref WEBSOCKET_SENT: IntCounterVec = IntCounterVec::new(
        Opts::new(
            "spacetime_websocket_sent",
            "Number of websocket messages sent to client"
        ),
        &["identity"]
    )
    .unwrap();
    pub static ref WEBSOCKET_SENT_MSG_SIZE: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_websocket_sent_msg_size",
            "The size of messages sent to connected sessions"
        ),
        &["identity"]
    )
    .unwrap();
    pub static ref PROCESS_CPU_USAGE: Gauge =
        Gauge::new("spacetime_worker_process_cpu_usage", "CPU usage of the worker process.").unwrap();
    pub static ref REDUCER_COUNT: IntCounterVec = IntCounterVec::new(
        Opts::new("spacetime_worker_transactions", "Number of reducer calls."),
        &["database_address", "reducer_symbol"]
    )
    .unwrap();
    pub static ref REDUCER_COMPUTE_TIME: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_worker_module_tx_compute_time",
            "The time it takes to compute and commit after reducer execution."
        ),
        &["database_address", "reducer_symbol"]
    )
    .unwrap();
    pub static ref REDUCER_WRITE_SIZE: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_worker_tx_size",
            "The size of committed bytes in the message log after reducer execution."
        ),
        &["database_address", "reducer_symbol"]
    )
    .unwrap();
    pub static ref NODE_IDENTITY_ENERGY_BUDGET_GAUGE: GaugeVec = GaugeVec::new(
        Opts::new(
            "spacetime_worker_identity_energy_budget",
            "Node-level energy budget, per identity"
        ),
        &["identity", "node"]
    )
    .unwrap();
    pub static ref INSTANCE_ENV_INSERT: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_instance_env_insert",
            "Time spent by reducers inserting rows (InstanceEnv::insert)"
        ),
        &["database_address", "table_id"]
    )
    .unwrap();
    pub static ref INSTANCE_ENV_DELETE_PK: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_instance_env_delete_pk",
            "Time spent by reducers deleting rows by pk (InstanceEnv::delete_pk)"
        ),
        &["database_address", "table_id"]
    )
    .unwrap();
    pub static ref INSTANCE_ENV_DELETE_VALUE: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_instance_env_delete_value",
            "Time spent by reducers deleting rows (InstanceEnv::delete_value)"
        ),
        &["database_address", "table_id"]
    )
    .unwrap();
    pub static ref INSTANCE_ENV_DELETE_EQ: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_instance_env_delete_eq",
            "Time spent by reducers deleting rows by eq (InstanceEnv::delete_eq)"
        ),
        &["database_address", "table_id"]
    )
    .unwrap();
    pub static ref INSTANCE_ENV_DELETE_RANGE: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_instance_env_delete_range",
            "Time spent by reducers deleting rows ranges eq (InstanceEnv::delete_range)"
        ),
        &["database_address", "table_id"]
    )
    .unwrap();
}

pub fn register_custom_metrics() {
    REGISTRY.register(Box::new(CONNECTED_CLIENTS.clone())).unwrap();
    REGISTRY.register(Box::new(WEBSOCKET_REQUESTS.clone())).unwrap();
    REGISTRY.register(Box::new(WEBSOCKET_REQUEST_MSG_SIZE.clone())).unwrap();
    REGISTRY.register(Box::new(WEBSOCKET_SENT.clone())).unwrap();
    REGISTRY.register(Box::new(WEBSOCKET_SENT_MSG_SIZE.clone())).unwrap();
    REGISTRY.register(Box::new(PROCESS_CPU_USAGE.clone())).unwrap();
    REGISTRY.register(Box::new(REDUCER_COUNT.clone())).unwrap();
    REGISTRY.register(Box::new(REDUCER_COMPUTE_TIME.clone())).unwrap();
    REGISTRY.register(Box::new(REDUCER_WRITE_SIZE.clone())).unwrap();
    REGISTRY.register(Box::new(INSTANCE_ENV_INSERT.clone())).unwrap();
    REGISTRY.register(Box::new(INSTANCE_ENV_DELETE_PK.clone())).unwrap();
    REGISTRY.register(Box::new(INSTANCE_ENV_DELETE_VALUE.clone())).unwrap();
    REGISTRY.register(Box::new(INSTANCE_ENV_DELETE_EQ.clone())).unwrap();
    REGISTRY.register(Box::new(INSTANCE_ENV_DELETE_RANGE.clone())).unwrap();
    REGISTRY
        .register(Box::new(NODE_IDENTITY_ENERGY_BUDGET_GAUGE.clone()))
        .unwrap();
}
