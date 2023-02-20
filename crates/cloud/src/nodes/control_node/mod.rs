use std::sync::Arc;

use spacetimedb::control_db::CONTROL_DB;
use spacetimedb::database_instance_context_controller::DatabaseInstanceContextController;
use spacetimedb::object_db::ObjectDb;
use tokio::task::JoinHandle;

mod budget_controller;
pub(crate) mod client_api; // TODO: should be private
mod controller;
pub(crate) mod prometheus_metrics;
pub(crate) mod worker_api;

use controller::Controller;

pub async fn start(
    db_inst_ctx_controller: Arc<DatabaseInstanceContextController>,
    config: crate::nodes::node_config::ControlNodeConfig,
) -> [JoinHandle<()>; 2] {
    prometheus_metrics::register_custom_metrics();

    // Load energy balances and set up budget allocations for all nodes.
    budget_controller::BUDGET_CONTROLLER
        .refresh_all_budget_allocations()
        .await;

    let client_api_addr = config.client_api_listen_addr.clone();
    let object_db = Arc::new(ObjectDb::init().unwrap());
    let controller = Controller::new(&CONTROL_DB, object_db);
    let controller2 = controller.clone();
    [
        tokio::spawn(async { worker_api::start(controller, config).await }),
        tokio::spawn(async { client_api::start(db_inst_ctx_controller, controller2, client_api_addr).await }),
    ]
}
