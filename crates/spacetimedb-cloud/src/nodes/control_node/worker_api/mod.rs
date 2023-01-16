pub(crate) mod routes;
pub(crate) mod worker_connection;
pub(crate) mod worker_connection_index;
use routes::router;
use tokio::net::TcpListener;
use worker_connection_index::WorkerConnectionIndex;

pub async fn start(config: crate::nodes::node_config::ControlNodeConfig) -> ! {
    WorkerConnectionIndex::start_liveliness_check();
    WorkerConnectionIndex::start_worker_budget_update();

    let listener = TcpListener::bind(config.worker_api_listen_addr).await.unwrap();

    log::debug!(
        "Control node worker API listening for http requests at http://{}",
        listener.local_addr().unwrap()
    );
    gotham::bind_server(listener, router(), futures::future::ok).await
}
