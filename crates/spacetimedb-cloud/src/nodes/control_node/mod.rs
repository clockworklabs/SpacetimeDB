use tokio::task::JoinHandle;

mod budget_controller;
pub(crate) mod client_api; // TODO: should be private
mod controller;
pub(crate) mod prometheus_metrics;
pub(crate) mod worker_api;

pub async fn start(config: crate::nodes::node_config::ControlNodeConfig) -> [JoinHandle<()>; 2] {
    prometheus_metrics::register_custom_metrics();

    // Load energy balances and set up budget allocations for all nodes.
    budget_controller::refresh_all_budget_allocations().await;

    let client_api_addr = config.client_api_listen_addr.clone();
    [
        tokio::spawn(async { worker_api::start(config).await }),
        tokio::spawn(async { client_api::start(client_api_addr).await }),
    ]
}
