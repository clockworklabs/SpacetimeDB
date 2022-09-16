use lazy_static::lazy_static;
use prometheus::{Gauge, HistogramOpts, HistogramVec, IntCounterVec, IntGauge, Opts, Registry};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    pub static ref CONNECTED_CLIENTS: IntGauge = IntGauge::new(
        "spacetime_worker_connected_clients",
        "Number of clients connected to the worker."
    )
    .unwrap();
    pub static ref PROCESS_CPU_USAGE: Gauge =
        Gauge::new("spacetime_worker_process_cpu_usage", "CPU usage of the worker process.").unwrap();
    pub static ref TX_COUNT: IntCounterVec = IntCounterVec::new(
        Opts::new("spacetime_worker_transactions", "Number of transactions."),
        &["database_address", "reducer_symbol"]
    )
    .unwrap();
    pub static ref TX_COMPUTE_TIME: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_worker_module_tx_compute_time",
            "The time it takes to compute and commit the transaction."
        ),
        &["database_address", "reducer_symbol"]
    )
    .unwrap();
    pub static ref TX_SIZE: HistogramVec = HistogramVec::new(
        HistogramOpts::new(
            "spacetime_worker_tx_size",
            "The size of the transaction in the message log."
        ),
        &["database_address", "reducer_symbol"]
    )
    .unwrap();
}

pub fn register_custom_metrics() {
    REGISTRY.register(Box::new(CONNECTED_CLIENTS.clone())).unwrap();

    REGISTRY.register(Box::new(PROCESS_CPU_USAGE.clone())).unwrap();

    REGISTRY.register(Box::new(TX_COUNT.clone())).unwrap();

    REGISTRY.register(Box::new(TX_COMPUTE_TIME.clone())).unwrap();

    REGISTRY.register(Box::new(TX_SIZE.clone())).unwrap();
}
