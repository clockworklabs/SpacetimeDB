use super::terminal_tests::bindings::RemoteModule;
use super::*;
use std::{net::SocketAddr, time::Duration};
use tokio::{io::AsyncReadExt, net::TcpListener, sync::oneshot, task::JoinHandle};

async fn stalled_handshake_peer() -> (SocketAddr, oneshot::Receiver<()>, JoinHandle<()>) {
    // This owned numeric-loopback listener never completes the HTTP upgrade.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (ready_tx, ready_rx) = oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut socket, remote) = listener.accept().await.unwrap();
        assert!(remote.ip().is_loopback());
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = socket.read(&mut buffer).await.unwrap();
            assert_ne!(count, 0, "client closed before sending the upgrade request");
            request.extend_from_slice(&buffer[..count]);
            assert!(request.len() <= 16 * 1024);
        }
        ready_tx.send(()).unwrap();
        assert_eq!(
            socket.read(&mut buffer).await.unwrap(),
            0,
            "pending handshake socket was retained"
        );
    });
    (address, ready_rx, peer)
}

async fn check_pending_handshake_cleanup(use_timeout: bool) {
    let (address, ready, peer) = stalled_handshake_peer().await;
    let capture = Arc::new(());
    let weak_capture = Arc::downgrade(&capture);
    let mut connect = Box::pin(
        DbConnectionBuilder::<RemoteModule>::new()
            .with_uri(format!("http://{address}"))
            .with_database_name("disposable-builder-handshake-test")
            .on_connect(move |_, _, _| drop(capture))
            .build_async(),
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        tokio::select! {
            result = &mut connect => panic!("stalled handshake unexpectedly completed: {:?}", result.err()),
            result = ready => result.unwrap(),
        }
    })
    .await
    .expect("async builder blocked the executor or never initiated its handshake");
    assert!(weak_capture.upgrade().is_some());

    if use_timeout {
        assert!(tokio::time::timeout(Duration::from_millis(30), connect).await.is_err());
    } else {
        drop(connect);
    }
    assert!(
        weak_capture.upgrade().is_none(),
        "cancelled builder retained callback captures"
    );
    tokio::time::timeout(Duration::from_secs(2), peer)
        .await
        .expect("cancelled handshake did not close its socket")
        .unwrap();
}

// A current-thread runtime also proves the asynchronous builder does not use
// block_in_place or block_on while waiting for the peer's handshake response.
#[tokio::test]
async fn async_builder_timeout_closes_stalled_handshake_and_releases_callbacks() {
    check_pending_handshake_cleanup(true).await;
}

#[tokio::test]
async fn async_builder_cancellation_closes_stalled_handshake_and_releases_callbacks() {
    check_pending_handshake_cleanup(false).await;
}

#[test]
fn sync_builder_returns_connect_errors_without_dropping_its_runtime_inside_block_on() {
    // The SDK rejects a query in the host URI before attempting any network I/O.
    // Running outside Tokio exercises the synchronous API's owned runtime.
    let result = DbConnectionBuilder::<RemoteModule>::new()
        .with_uri("http://127.0.0.1:1/?unexpected=query")
        .with_database_name("disposable-builder-error-test")
        .build();
    assert!(matches!(result, Err(crate::Error::FailedToConnect { .. })));
}

#[tokio::test]
async fn container_builds_fetch_fresh_tokens_and_send_only_to_the_concrete_target() {
    use crate::credentials::{container_tests as fixture, Container};
    use tokio::io::AsyncWriteExt;
    let (broker, broker_task) = fixture::http_fixture(vec![
        fixture::response(200, &fixture::token_body("first-private-token", 20)),
        fixture::response(200, &fixture::token_body("second-private-token", 20)),
    ])
    .await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_uri = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        for expected in ["first-private-token", "second-private-token"] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let request = fixture::read_request(&mut socket).await;
            assert!(request.starts_with(&format!("GET /v1/database/{}/subscribe?", fixture::SOURCE)));
            assert!(request
                .to_ascii_lowercase()
                .contains(&format!("authorization: bearer {expected}\r\n")));
            // Deliberately echo secret material in a failed upgrade. Hosted
            // connection errors must not retain the response or its headers.
            socket.write_all(&fixture::response(403, expected)).await.unwrap();
            socket.shutdown().await.unwrap();
        }
    });
    let container = Container::new(fixture::SOURCE, &server_uri, &broker).unwrap();
    for _ in 0..2 {
        let error = DbConnectionBuilder::<RemoteModule>::new()
            .with_container_credentials(container.clone())
            .with_database_name("self")
            .build_async()
            .await
            .err()
            .expect("fixture rejects the WebSocket upgrade");
        assert!(!format!("{error:?} {error}").contains("private-token"));
    }
    assert_eq!(broker_task.await.unwrap().len(), 2);
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn container_auth_rejects_static_credentials_and_debug_files_before_io() {
    use crate::credentials::{container_tests as fixture, Container};
    let container = Container::new(
        fixture::SOURCE,
        "https://must-not-contact.invalid",
        "http://127.0.0.1:1/v1/credentials",
    )
    .unwrap();
    let path = std::env::temp_dir().join(format!("must-not-write-sdk-credentials-{}", std::process::id()));
    assert!(!path.exists());
    let result = DbConnectionBuilder::<RemoteModule>::new()
        .with_token(Some("owner-secret"))
        .with_container_credentials(container.clone())
        .build_async()
        .await;
    let error = result.err().unwrap();
    assert!(error.to_string().contains("static token"));
    assert!(!format!("{error:?} {error}").contains("owner-secret"));
    let result = DbConnectionBuilder::<RemoteModule>::new()
        .with_container_credentials(container)
        .with_debug_to_file(&path)
        .build_async()
        .await;
    assert!(result.err().unwrap().to_string().contains("debug files"));
    assert!(!path.exists());
}

#[tokio::test]
async fn cancelling_container_build_closes_broker_socket_and_releases_callbacks() {
    use crate::credentials::{container_tests as fixture, Container};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let broker = format!("http://{}/v1/credentials", listener.local_addr().unwrap());
    let container = Container::new(fixture::SOURCE, "https://must-not-contact.invalid", &broker).unwrap();
    let (ready_tx, ready_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        fixture::read_request(&mut socket).await;
        ready_tx.send(()).unwrap();
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), socket.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    });
    let capture = Arc::new(());
    let weak = Arc::downgrade(&capture);
    let mut connect = Box::pin(
        DbConnectionBuilder::<RemoteModule>::new()
            .with_container_credentials(container)
            .on_connect(move |_, _, _| drop(capture))
            .build_async(),
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        tokio::select! {
            _ = &mut connect => panic!("stalled broker unexpectedly answered"),
            ready = ready_rx => ready.unwrap(),
        }
    })
    .await
    .unwrap();
    drop(connect);
    assert!(weak.upgrade().is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn container_token_expiry_closes_a_stalled_websocket_handshake() {
    use crate::credentials::{container_tests as fixture, Container};
    let (broker, broker_task) =
        fixture::http_fixture(vec![fixture::response(200, &fixture::token_body("expiring-token", 1))]).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_uri = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        fixture::read_request(&mut socket).await;
        let mut byte = [0];
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), socket.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    });
    let container = Container::new(fixture::SOURCE, &server_uri, &broker).unwrap();
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        DbConnectionBuilder::<RemoteModule>::new()
            .with_container_credentials(container)
            .build_async(),
    )
    .await
    .unwrap()
    .err()
    .unwrap();
    assert!(error.to_string().contains("expired"));
    broker_task.await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
}
