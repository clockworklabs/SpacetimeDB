use once_cell::sync::Lazy;
use prometheus::{Gauge, GaugeVec, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry};

pub struct WorkerMetrics {
    registry: Registry,
    connected_clients: IntGauge,
    websocket_requests: IntCounterVec,
    websocket_request_msg_size: HistogramVec,
    websocket_sent: IntCounterVec,
    websocket_sent_msg_size: HistogramVec,
    process_cpu_usage: Gauge,
    reducer_count: IntCounterVec,
    reducer_compute_time: HistogramVec,
    reducer_write_size: HistogramVec,
    node_identity_energy_budget_gauge: GaugeVec,
    instance_env_insert: HistogramVec,
    instance_env_delete_pk: HistogramVec,
    instance_env_delete_value: HistogramVec,
    instance_env_delete_eq: HistogramVec,
    instance_env_delete_range: HistogramVec,
}

static WORKER_METRICS: Lazy<WorkerMetrics> = Lazy::new(WorkerMetrics::new);

impl WorkerMetrics {
    fn new() -> Self {
        Self {
            registry: Registry::new(),
            connected_clients: IntGauge::new(
                "spacetime_worker_connected_clients",
                "Number of clients connected to the worker.",
            )
            .unwrap(),
            websocket_requests: IntCounterVec::new(
                Opts::new("spacetime_websocket_requests", "Number of websocket request messages"),
                &["instance_id", "protocol"],
            )
            .unwrap(),
            websocket_request_msg_size: HistogramVec::new(
                HistogramOpts::new(
                    "spacetime_websocket_request_msg_size",
                    "The size of messages received on connected sessions",
                ),
                &["instance_id", "protocol"],
            )
            .unwrap(),
            websocket_sent: IntCounterVec::new(
                Opts::new(
                    "spacetime_websocket_sent",
                    "Number of websocket messages sent to client",
                ),
                &["identity"],
            )
            .unwrap(),
            websocket_sent_msg_size: HistogramVec::new(
                HistogramOpts::new(
                    "spacetime_websocket_sent_msg_size",
                    "The size of messages sent to connected sessions",
                ),
                &["identity"],
            )
            .unwrap(),
            process_cpu_usage: Gauge::new("spacetime_worker_process_cpu_usage", "CPU usage of the worker process.")
                .unwrap(),
            reducer_count: IntCounterVec::new(
                Opts::new("spacetime_worker_transactions", "Number of reducer calls."),
                &["database_address", "reducer_symbol"],
            )
            .unwrap(),
            reducer_compute_time: HistogramVec::new(
                HistogramOpts::new(
                    "spacetime_worker_module_tx_compute_time",
                    "The time it takes to compute and commit after reducer execution.",
                ),
                &["database_address", "reducer_symbol"],
            )
            .unwrap(),
            reducer_write_size: HistogramVec::new(
                HistogramOpts::new(
                    "spacetime_worker_tx_size",
                    "The size of committed bytes in the message log after reducer execution.",
                ),
                &["database_address", "reducer_symbol"],
            )
            .unwrap(),
            node_identity_energy_budget_gauge: GaugeVec::new(
                Opts::new(
                    "spacetime_worker_identity_energy_budget",
                    "Node-level energy budget, per identity",
                ),
                &["identity", "node"],
            )
            .unwrap(),
            instance_env_insert: HistogramVec::new(
                HistogramOpts::new(
                    "spacetime_instance_env_insert",
                    "Time spent by reducers inserting rows (InstanceEnv::insert)",
                ),
                &["database_address", "table_id"],
            )
            .unwrap(),
            instance_env_delete_pk: HistogramVec::new(
                HistogramOpts::new(
                    "spacetime_instance_env_delete_pk",
                    "Time spent by reducers deleting rows by pk (InstanceEnv::delete_pk)",
                ),
                &["database_address", "table_id"],
            )
            .unwrap(),
            instance_env_delete_value: HistogramVec::new(
                HistogramOpts::new(
                    "spacetime_instance_env_delete_value",
                    "Time spent by reducers deleting rows (InstanceEnv::delete_value)",
                ),
                &["database_address", "table_id"],
            )
            .unwrap(),
            instance_env_delete_eq: HistogramVec::new(
                HistogramOpts::new(
                    "spacetime_instance_env_delete_eq",
                    "Time spent by reducers deleting rows by eq (InstanceEnv::delete_eq)",
                ),
                &["database_address", "table_id"],
            )
            .unwrap(),
            instance_env_delete_range: HistogramVec::new(
                HistogramOpts::new(
                    "spacetime_instance_env_delete_range",
                    "Time spent by reducers deleting rows ranges eq (InstanceEnv::delete_range)",
                ),
                &["database_address", "table_id"],
            )
            .unwrap(),
        }
    }

    pub fn register_custom_metrics(&self) {
        self.registry
            .register(Box::new(self.connected_clients.clone()))
            .unwrap();
        self.registry
            .register(Box::new(self.websocket_requests.clone()))
            .unwrap();
        self.registry
            .register(Box::new(self.websocket_request_msg_size.clone()))
            .unwrap();
        self.registry.register(Box::new(self.websocket_sent.clone())).unwrap();
        self.registry
            .register(Box::new(self.websocket_sent_msg_size.clone()))
            .unwrap();
        self.registry
            .register(Box::new(self.process_cpu_usage.clone()))
            .unwrap();
        self.registry.register(Box::new(self.reducer_count.clone())).unwrap();
        self.registry
            .register(Box::new(self.reducer_compute_time.clone()))
            .unwrap();
        self.registry
            .register(Box::new(self.reducer_write_size.clone()))
            .unwrap();
        self.registry
            .register(Box::new(self.instance_env_insert.clone()))
            .unwrap();
        self.registry
            .register(Box::new(self.instance_env_delete_pk.clone()))
            .unwrap();
        self.registry
            .register(Box::new(self.instance_env_delete_value.clone()))
            .unwrap();
        self.registry
            .register(Box::new(self.instance_env_delete_eq.clone()))
            .unwrap();
        self.registry
            .register(Box::new(self.instance_env_delete_range.clone()))
            .unwrap();
        self.registry
            .register(Box::new(self.node_identity_energy_budget_gauge.clone()))
            .unwrap();
    }
}

use WORKER_METRICS as METRICS;
metrics_delegator!(REGISTRY, registry: Registry);
metrics_delegator!(CONNECTED_CLIENTS, connected_clients: IntGauge);
metrics_delegator!(WEBSOCKET_REQUESTS, websocket_requests: IntCounterVec);
metrics_delegator!(WEBSOCKET_REQUEST_MSG_SIZE, websocket_request_msg_size: HistogramVec);
metrics_delegator!(WEBSOCKET_SENT, websocket_sent: IntCounterVec);
metrics_delegator!(WEBSOCKET_SENT_MSG_SIZE, websocket_sent_msg_size: HistogramVec);
metrics_delegator!(PROCESS_CPU_USAGE, process_cpu_usage: Gauge);
metrics_delegator!(REDUCER_COUNT, reducer_count: IntCounterVec);
metrics_delegator!(REDUCER_COMPUTE_TIME, reducer_compute_time: HistogramVec);
metrics_delegator!(REDUCER_WRITE_SIZE, reducer_write_size: HistogramVec);
metrics_delegator!(
    NODE_IDENTITY_ENERGY_BUDGET_GAUGE,
    node_identity_energy_budget_gauge: GaugeVec
);
metrics_delegator!(INSTANCE_ENV_INSERT, instance_env_insert: HistogramVec);
metrics_delegator!(INSTANCE_ENV_DELETE_PK, instance_env_delete_pk: HistogramVec);
metrics_delegator!(INSTANCE_ENV_DELETE_VALUE, instance_env_delete_value: HistogramVec);
metrics_delegator!(INSTANCE_ENV_DELETE_EQ, instance_env_delete_eq: HistogramVec);
metrics_delegator!(INSTANCE_ENV_DELETE_RANGE, instance_env_delete_range: HistogramVec);

pub fn register_custom_metrics() {
    WORKER_METRICS.register_custom_metrics()
}
