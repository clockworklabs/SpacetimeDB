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
