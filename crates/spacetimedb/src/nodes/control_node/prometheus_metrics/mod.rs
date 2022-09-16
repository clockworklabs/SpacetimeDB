use lazy_static::lazy_static;
use prometheus::{IntGauge, Registry};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    pub static ref WORKER_NODE_COUNT: IntGauge =
        IntGauge::new("spacetime_control_worker_nodes", "Worker Nodes").unwrap();
}

pub fn register_custom_metrics() {
    REGISTRY.register(Box::new(WORKER_NODE_COUNT.clone())).unwrap();
}
