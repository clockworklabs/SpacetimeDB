// mod client_api;
// mod logs;
mod control_node_connection;
mod worker_db;

use tokio::spawn;

pub async fn start(config: crate::nodes::node_config::NodeConfig) {
    // client_api::clients::init_connections();

    let bootstrap_addr = config.worker_node.as_ref().unwrap().bootstrap_addrs.first().unwrap().clone();
    spawn(async move {
        control_node_connection::start(bootstrap_addr).await;
    });

    // let client_listen_addr = config.worker_node.as_ref().unwrap().listen_addr.clone();
    // spawn(async move {
    //     // TODO(cloutiertyler): remove this as we are using the control nodes to allocate identities
    //     postgres::init().await;
    //     client_api::start(client_listen_addr).await;
    // }).await.unwrap();
}