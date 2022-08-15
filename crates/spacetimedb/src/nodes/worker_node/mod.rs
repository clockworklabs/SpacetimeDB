// mod logs;
mod control_node_connection;
mod worker_db;
mod wasm_host_controller;
mod wasm_instance_env;
mod wasm_module_host;
mod client_api;
mod database_logger;
mod worker_database_instance;

use tokio::spawn;

pub async fn start(config: crate::nodes::node_config::NodeConfig) {
    let bootstrap_addr = config.worker_node.as_ref().unwrap().bootstrap_addrs.first().unwrap().clone();
    spawn(async move {
        control_node_connection::start(bootstrap_addr).await;
    });

    let client_listen_addr = config.worker_node.as_ref().unwrap().listen_addr.clone();
    spawn(async move {
        client_api::start(client_listen_addr).await;
    }).await.unwrap();
}