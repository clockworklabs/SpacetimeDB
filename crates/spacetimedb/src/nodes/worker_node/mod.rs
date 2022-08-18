// mod logs;
mod client_api;
mod control_node_connection;
mod database_logger;
mod wasm_host_controller;
mod wasm_instance_env;
mod wasm_module_host;
mod worker_database_instance;
mod worker_db;

use tokio::spawn;

pub async fn start(config: crate::nodes::node_config::NodeConfig) {
    let worker_api_bootstrap_addr = config
        .worker_node
        .as_ref()
        .unwrap()
        .worker_api_bootstrap_addrs
        .first()
        .unwrap()
        .clone();
    let client_api_bootstrap_addr = config
        .worker_node
        .as_ref()
        .unwrap()
        .client_api_bootstrap_addrs
        .first()
        .unwrap()
        .clone();
    spawn(async move {
        control_node_connection::start(worker_api_bootstrap_addr, client_api_bootstrap_addr).await;
    });

    let client_listen_addr = config.worker_node.as_ref().unwrap().listen_addr.clone();
    spawn(async move {
        client_api::start(client_listen_addr).await;
    })
    .await
    .unwrap();
}
