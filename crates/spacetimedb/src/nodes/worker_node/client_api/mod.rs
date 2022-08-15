pub mod client_connection;
pub mod client_connection_index;
pub mod module_subscription_actor;
use std::net::SocketAddr;
mod routes;

use routes::router;

pub async fn start(listen_addr: String) {
    client_connection_index::ClientActorIndex::start_liveliness_check();

    let addr: SocketAddr = listen_addr.parse().unwrap();
    log::debug!("Worker client API listening for http requests at http://{}", addr);
    gotham::init_server(addr, router()).await.unwrap();
}