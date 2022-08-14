pub(crate) mod routes;
mod worker_connection;
pub(crate) mod worker_connection_index;
use std::net::SocketAddr;
use tokio::spawn;
use routes::router;
use worker_connection_index::WorkerConnectionIndex;

pub async fn start(config: crate::nodes::node_config::NodeConfig) {
    WorkerConnectionIndex::start_liveliness_check();
    spawn(async move {
        let listen_addr: SocketAddr = config.control_node.as_ref().unwrap().worker_api_listen_addr.parse().unwrap();

        log::debug!("Control node worker API listening for http requests at http://{}", listen_addr);
        gotham::init_server(listen_addr, router()).await.unwrap();
    }).await.unwrap();
}