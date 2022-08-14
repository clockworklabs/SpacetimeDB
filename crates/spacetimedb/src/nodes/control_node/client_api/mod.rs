mod routes;
use std::net::SocketAddr;
use tokio::spawn;
use routes::router;

pub async fn start(port: u16) {
    spawn(async move {
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        log::debug!("Control node client API listening for http requests at http://{}", addr);
        gotham::init_server(addr, router()).await.unwrap();
    }).await.unwrap();
}