// mod logs;
mod client_api;
pub(crate) mod control_node_connection;
pub(crate) mod worker_budget;
mod worker_db;
mod worker_metrics;

// use perf_monitor::cpu::ProcessStat;
use spacetimedb::db::db_metrics;
use tokio::task::JoinHandle;

// use crate::nodes::worker_node::db_metrics::PROCESS_CPU_USAGE;

pub async fn start(config: crate::nodes::node_config::WorkerNodeConfig) -> [JoinHandle<()>; 2] {
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
        tokio::spawn(control_node_connection::start(config)),
        tokio::spawn(async move { client_api::start(client_listen_addr).await }),
    ]
}
