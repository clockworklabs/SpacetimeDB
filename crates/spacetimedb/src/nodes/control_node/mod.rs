pub(crate) mod client_api; // TODO: should be private
mod control_budget;
pub(crate) mod control_db;
mod controller;
mod object_db;
pub(crate) mod prometheus_metrics;
pub(crate) mod worker_api;

use futures::{future::join_all, FutureExt};

pub async fn start(config: crate::nodes::node_config::NodeConfig) {
    prometheus_metrics::register_custom_metrics();

    // Load energy balances and set up budget allocations for all nodes.
    control_budget::refresh_all_budget_allocations().await;

    join_all(vec![
        worker_api::start(config).boxed(),
        client_api::start(26258).boxed(),
    ])
    .await;
}
