use spacetimedb::client::client_connection_index;
mod proxy;
mod routes;
use std::net::SocketAddr;

use routes::router;

pub async fn start(listen_addr: String) {
    client_connection_index::ClientActorIndex::start_liveliness_check();

    let addr: SocketAddr = listen_addr.parse().unwrap();
    log::debug!("Starting client API listening on {}", addr);
    gotham::init_server(addr, router()).await.unwrap();
}
