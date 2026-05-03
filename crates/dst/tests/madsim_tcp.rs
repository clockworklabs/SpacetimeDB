#![cfg(madsim)]

use std::{net::SocketAddr, sync::Arc};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Barrier,
};

#[test]
fn tcp_round_trip_over_madsim_tokio() {
    let runtime = madsim::runtime::Runtime::new();
    let server_addr: SocketAddr = "10.0.0.1:1".parse().unwrap();
    let client_addr: SocketAddr = "10.0.0.2:1".parse().unwrap();

    let server = runtime.create_node().ip(server_addr.ip()).build();
    let client = runtime.create_node().ip(client_addr.ip()).build();
    let ready = Arc::new(Barrier::new(2));

    let server_ready = ready.clone();
    let server_task = server.spawn(async move {
        let listener = tokio::net::TcpListener::bind(server_addr).await.unwrap();
        server_ready.wait().await;
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"pong").await.unwrap();
        stream.flush().await.unwrap();
    });

    let client_task = client.spawn(async move {
        ready.wait().await;
        let mut stream = tokio::net::TcpStream::connect(server_addr).await.unwrap();
        let mut response = [0; 4];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(&response, b"pong");
    });

    runtime.block_on(server_task).unwrap();
    runtime.block_on(client_task).unwrap();
}
