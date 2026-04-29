use std::{net::SocketAddr, time::Duration};

use axum::{routing::get, Router};

#[test]
fn axum_server_reqwest_client_over_madsim_tcp() {
    let runtime = madsim::runtime::Runtime::with_seed_and_config(1, madsim::Config::default());
    let server_addr: SocketAddr = "10.0.0.1:3000".parse().unwrap();
    let client_addr: SocketAddr = "10.0.0.2:0".parse().unwrap();

    let server = runtime.create_node().ip(server_addr.ip()).build();
    let client = runtime.create_node().ip(client_addr.ip()).build();
    let ready = std::sync::Arc::new(tokio::sync::Barrier::new(2));

    let server_ready = ready.clone();
    server.spawn(async move {
        let app = Router::new().route("/ping", get(|| async { "pong" }));
        let listener = tokio::net::TcpListener::bind(server_addr).await.unwrap();
        server_ready.wait().await;
        axum::serve(listener, app).await.unwrap();
    });

    let client_task = client.spawn(async move {
        ready.wait().await;
        let url = format!("http://{server_addr}/ping");
        let body = reqwest::get(url).await.unwrap().text().await.unwrap();
        assert_eq!(body, "pong");
    });

    runtime.block_on(async move {
        tokio::time::timeout(Duration::from_secs(5), client_task)
            .await
            .unwrap()
            .unwrap();
    });
}
