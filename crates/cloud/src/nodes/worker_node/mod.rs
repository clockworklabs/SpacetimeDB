// mod logs;
mod client_api;
pub(crate) mod control_node_connection;
pub(crate) mod worker_budget;
mod worker_db;
mod worker_metrics;

use std::sync::Arc;

use spacetimedb::database_instance_context_controller::DatabaseInstanceContextController;
// use perf_monitor::cpu::ProcessStat;
use spacetimedb::db::db_metrics;
use tokio::task::JoinHandle;

// use crate::nodes::worker_node::db_metrics::PROCESS_CPU_USAGE;

pub async fn start(
    db_inst_ctx_controller: Arc<DatabaseInstanceContextController>,
    config: crate::nodes::node_config::WorkerNodeConfig,
) -> [JoinHandle<()>; 2] {
    let client_listen_addr = config.listen_addr.clone();

    // Metrics for pieces under worker_node/ related to reducer hosting, etc.
    worker_metrics::register_custom_metrics();

    // Metrics for our use of db/.
    db_metrics::register_custom_metrics();

    // spawn(async move {
    //     let mut stat = ProcessStat::cur().unwrap();
    //     loop {
    //         let usage = stat.cpu().unwrap();
    //         PROCESS_CPU_USAGE.set(usage);
    //         tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    //     }
    // });

    [
        tokio::spawn(control_node_connection::start(db_inst_ctx_controller.clone(), config)),
        tokio::spawn(async move { client_api::start(db_inst_ctx_controller, client_listen_addr).await }),
    ]
}
