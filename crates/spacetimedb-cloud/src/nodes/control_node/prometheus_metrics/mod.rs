use lazy_static::lazy_static;
use prometheus::{GaugeVec, IntGauge, Opts, Registry};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    pub static ref WORKER_NODE_COUNT: IntGauge =
        IntGauge::new("spacetime_control_worker_nodes", "Worker Nodes").unwrap();
    pub static ref IDENTITY_ENERGY_BALANCE_GAUGE: GaugeVec = GaugeVec::new(
        Opts::new(
            "spacetime_control_identity_energy_balance",
            "Top level energy balance, per identity"
        ),
        &["identity"]
    )
    .unwrap();
}

pub fn register_custom_metrics() {
    REGISTRY.register(Box::new(WORKER_NODE_COUNT.clone())).unwrap();
    REGISTRY
        .register(Box::new(IDENTITY_ENERGY_BALANCE_GAUGE.clone()))
        .unwrap();
}
