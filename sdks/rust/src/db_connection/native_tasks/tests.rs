use super::*;
use crate::db_connection::{
    build_db_ctx, build_db_ctx_inner, terminal_tests::bindings::RemoteModule, DbConnectionBuilder, ParsedMessage,
};
use futures::{FutureExt, SinkExt, StreamExt};
use spacetimedb_client_api_messages::websocket::v2 as ws;
use spacetimedb_lib::{bsatn, ConnectionId, Identity, Timestamp};
use std::{panic::AssertUnwindSafe, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::oneshot};
use tokio_tungstenite::tungstenite::{handshake::server::Response, Message};

async fn blocked_task() -> (JoinHandle<()>, std::sync::mpsc::Sender<()>) {
    let (started, ready) = oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let task = tokio::task::spawn_blocking(move || {
        let _ = started.send(());
        // Dropping release on a failed assertion also releases this worker.
        let _ = blocked.recv();
    });
    ready.await.unwrap();
    (task, release)
}

#[tokio::test]
async fn cancellation_during_second_join_preserves_owner_for_retry() {
    let first = tokio::spawn(std::future::pending());
    let (second, release) = blocked_task().await;
    let mut tasks = NativeTasks::new(first, second);
    assert!(tokio::time::timeout(Duration::from_millis(30), tasks.stop_and_join())
        .await
        .is_err());
    assert!(tasks.websocket.is_none(), "first task should already have been joined");
    assert!(tasks.parser.is_some(), "pending task must still have an owner");
    release.send(()).unwrap();
    tasks.stop_and_join().await.unwrap();
    assert!(tasks.parser.is_none());
    tasks.stop_and_join().await.unwrap();
}

#[tokio::test]
async fn task_panic_still_joins_other_task_and_remains_an_error_after_retry() {
    let (panicking, ready) = oneshot::channel();
    let first = tokio::spawn(async move {
        let _ = panicking.send(());
        panic!("injected connection task panic");
    });
    ready.await.unwrap();
    let (second, release) = blocked_task().await;
    let mut tasks = NativeTasks::new(first, second);
    assert!(tokio::time::timeout(Duration::from_millis(30), tasks.stop_and_join())
        .await
        .is_err());
    assert!(tasks.failure.is_some());
    assert!(tasks.websocket.is_none());
    assert!(tasks.parser.is_some());
    release.send(()).unwrap();
    assert!(tasks.stop_and_join().await.is_err());
    assert!(tasks.parser.is_none());
    assert!(tasks.stop_and_join().await.is_err());
}

#[tokio::test]
async fn shutdown_drops_both_task_resources_before_returning() {
    let resource = Arc::new(());
    let first_capture = resource.clone();
    let second_capture = resource.clone();
    let first = tokio::spawn(async move {
        let _capture = first_capture;
        std::future::pending::<()>().await;
    });
    let second = tokio::spawn(async move {
        let _capture = second_capture;
        std::future::pending::<()>().await;
    });
    let mut tasks = NativeTasks::new(first, second);
    tasks.stop_and_join().await.unwrap();
    assert_eq!(Arc::strong_count(&resource), 1);
}

#[tokio::test]
async fn cancelled_terminal_driver_preserves_original_processing_error() {
    let first = tokio::spawn(std::future::pending());
    let (second, release) = blocked_task().await;
    let inner = build_db_ctx_inner::<RemoteModule>(None, NativeTasks::new(first, second), None, None, None);
    let (outgoing, _outgoing_recv) = futures_channel::mpsc::unbounded();
    let (incoming, incoming_recv) = futures_channel::mpsc::unbounded();
    let (pending, pending_recv) = futures_channel::mpsc::unbounded();
    incoming
        .unbounded_send(ParsedMessage::Error(
            InternalError::new("original processing failure").into(),
        ))
        .unwrap();
    drop(incoming);
    let context = build_db_ctx(
        tokio::runtime::Handle::current(),
        inner,
        outgoing,
        Arc::new(tokio::sync::Mutex::new(incoming_recv)),
        pending,
        Arc::new(tokio::sync::Mutex::new(pending_recv)),
        None,
        None,
    );
    assert!(tokio::time::timeout(Duration::from_millis(30), context.run_async())
        .await
        .is_err());
    release.send(()).unwrap();
    let error = context.run_async().await.unwrap_err();
    assert!(error.to_string().contains("original processing failure"));
}

async fn terminal_connection_joins_tasks(callback_panics: bool, poison_sender: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (closed, closure) = oneshot::channel();
    let peer = tokio::spawn(async move {
        let (socket, address) = listener.accept().await.unwrap();
        assert!(address.ip().is_loopback());
        let mut socket =
            tokio_tungstenite::accept_hdr_async(socket, |_: &http::Request<()>, mut response: Response| {
                response
                    .headers_mut()
                    .insert("sec-websocket-protocol", ws::BIN_PROTOCOL.parse().unwrap());
                Ok(response)
            })
            .await
            .unwrap();
        let message = if callback_panics {
            ws::ServerMessage::InitialConnection(ws::InitialConnection {
                identity: Identity::from_u256(1u32.into()),
                connection_id: ConnectionId::from_u128(1),
                token: "disposable-test-token".into(),
            })
        } else {
            // A well-formed response for a request this client never made is a
            // processing error. The peer deliberately keeps its socket open.
            ws::ServerMessage::ReducerResult(ws::ReducerResult {
                request_id: u32::MAX,
                timestamp: Timestamp::UNIX_EPOCH,
                result: ws::ReducerOutcome::OkEmpty,
            })
        };
        let mut bytes = vec![0]; // No compression.
        bytes.extend(bsatn::to_vec(&message).unwrap());
        socket.send(Message::Binary(bytes.into())).await.unwrap();
        while let Some(message) = socket.next().await {
            if message.is_err() || matches!(message, Ok(Message::Close(_))) {
                break;
            }
        }
        let _ = closed.send(());
    });
    // Keep a positive join even when the test body fails or times out.
    let result = AssertUnwindSafe(tokio::time::timeout(Duration::from_secs(3), async {
        let context = DbConnectionBuilder::<RemoteModule>::new()
            .with_uri(format!("http://{address}"))
            .with_database_name("disposable-native-task-test")
            .on_connect(move |_, _, _| {
                assert!(!callback_panics, "injected application callback panic");
            })
            .build_native_impl(tokio::runtime::Handle::current())
            .await
            .unwrap();
        if poison_sender {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let _outgoing = context.send_chan.lock().unwrap();
                panic!("injected outgoing queue panic while holding its mutex");
            }));
            assert!(result.is_err());
        }
        let result = AssertUnwindSafe(context.run_async()).catch_unwind().await;
        if callback_panics || poison_sender {
            assert!(result.is_err(), "callback panic should propagate after cleanup");
        } else {
            assert!(result.unwrap().is_err(), "unsolicited reducer result must fail");
        }
        assert!(context
            .send_chan
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_none());
        let tasks = context.inner.lock().unwrap().background_tasks.clone();
        let tasks = tasks.lock().await;
        assert!(tasks.websocket.is_none() && tasks.parser.is_none());
        closure.await.expect("peer disappeared before observing socket closure");
    }))
    .catch_unwind()
    .await;
    peer.abort();
    let joined = peer.await;
    if let Err(error) = joined {
        assert!(error.is_cancelled(), "test peer failed: {error}");
    }
    result.unwrap().expect("connection did not finish native task cleanup");
}

#[tokio::test]
async fn processing_error_closes_socket_and_joins_tasks_with_retained_connection() {
    terminal_connection_joins_tasks(false, false).await;
}

#[tokio::test]
async fn callback_panic_joins_tasks_before_propagating_unwind() {
    terminal_connection_joins_tasks(true, false).await;
}

#[tokio::test]
async fn poisoned_outgoing_mutex_does_not_bypass_native_task_join() {
    terminal_connection_joins_tasks(false, true).await;
}
