use super::super::{
    native_tasks::NativeTasks,
    terminal_tests::bindings::{self, RemoteModule},
};
use super::*;
use crate::{credentials::container_tests as fixture, DbContext, Table};
use bindings::{identity_connected, ConnectedTableAccess};
use futures::{SinkExt, StreamExt};
use spacetimedb_client_api_messages::websocket::{
    common::{BsatnRowList, RowSizeHint},
    v2 as ws,
};
use spacetimedb_lib::{bsatn, ConnectionId};
use std::{
    future::Future,
    sync::{atomic::AtomicUsize, Mutex},
    time::UNIX_EPOCH,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::timeout,
};
use tokio_tungstenite::{
    tungstenite::{handshake::server::Response, Message},
    WebSocketStream,
};

#[derive(spacetimedb_lib::ser::Serialize)]
#[sats(crate = spacetimedb_lib)]
struct EmptyArgs {}
impl crate::spacetime_module::InModule for EmptyArgs {
    type Module = RemoteModule;
}
use std::result::Result;

struct Task<T>(Option<JoinHandle<T>>);
impl<T> Task<T> {
    fn spawn(body: impl Future<Output = T> + Send + 'static) -> Self
    where
        T: Send + 'static,
    {
        Self(Some(tokio::spawn(body)))
    }
    async fn join(mut self) -> T {
        let result = timeout(Duration::from_secs(7), self.0.as_mut().unwrap())
            .await
            .unwrap()
            .unwrap();
        self.0 = None;
        result
    }
}
impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}

#[derive(Default)]
struct PeerStats {
    subscriptions: usize,
    reducers: usize,
    procedures: usize,
}

async fn send(socket: &mut WebSocketStream<tokio::net::TcpStream>, message: ws::ServerMessage) {
    let mut bytes = vec![0];
    bytes.extend(bsatn::to_vec(&message).unwrap());
    socket.send(Message::Binary(bytes.into())).await.unwrap();
}

async fn peer(sessions: usize, sender: Identity) -> (String, mpsc::UnboundedReceiver<usize>, Task<Vec<PeerStats>>) {
    peer_with_close(sessions, sender, false).await
}

async fn peer_with_close(
    sessions: usize,
    sender: Identity,
    close_initially: bool,
) -> (String, mpsc::UnboundedReceiver<usize>, Task<Vec<PeerStats>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let uri = format!("http://{}", listener.local_addr().unwrap());
    let (applied, rx) = mpsc::unbounded_channel();
    let task = Task::spawn(async move {
        timeout(Duration::from_secs(6), async {
            let mut stats = Vec::new();
            for generation in 1..=sessions {
                let (socket, remote) = listener.accept().await.unwrap();
                assert!(remote.ip().is_loopback());
                let mut socket = tokio_tungstenite::accept_hdr_async(
                    socket,
                    |request: &http::Request<()>, mut response: Response| {
                        assert!(request.uri().path().contains(fixture::SOURCE));
                        assert!(request.headers()["authorization"]
                            .to_str()
                            .unwrap()
                            .starts_with("Bearer disposable-"));
                        response
                            .headers_mut()
                            .insert("sec-websocket-protocol", ws::BIN_PROTOCOL.parse().unwrap());
                        Ok(response)
                    },
                )
                .await
                .unwrap();
                send(
                    &mut socket,
                    ws::ServerMessage::InitialConnection(ws::InitialConnection {
                        identity: sender,
                        connection_id: ConnectionId::from_u128(generation as u128),
                        token: "disposable-server-token".into(),
                    }),
                )
                .await;
                let mut current = PeerStats::default();
                if close_initially {
                    socket.close(None).await.unwrap();
                    stats.push(current);
                    continue;
                }
                while let Some(message) = socket.next().await {
                    let Ok(Message::Binary(bytes)) = message else {
                        break;
                    };
                    match bsatn::from_slice::<ws::ClientMessage>(&bytes).unwrap() {
                        ws::ClientMessage::Subscribe(subscribe) => {
                            current.subscriptions += 1;
                            let row = bsatn::to_vec(&bindings::Connected { identity: sender }).unwrap();
                            send(
                                &mut socket,
                                ws::ServerMessage::SubscribeApplied(ws::SubscribeApplied {
                                    request_id: subscribe.request_id,
                                    query_set_id: subscribe.query_set_id,
                                    rows: ws::QueryRows {
                                        tables: vec![ws::SingleTableRows {
                                            table: "connected".into(),
                                            rows: BsatnRowList::new(
                                                RowSizeHint::RowOffsets(vec![0].into()),
                                                row.into(),
                                            ),
                                        }]
                                        .into(),
                                    },
                                }),
                            )
                            .await;
                            let _ = applied.send(generation);
                        }
                        ws::ClientMessage::CallReducer(_) => current.reducers += 1,
                        ws::ClientMessage::CallProcedure(_) => current.procedures += 1,
                        _ => panic!("unexpected fixture request"),
                    }
                }
                stats.push(current);
            }
            stats
        })
        .await
        .expect("owned WebSocket fixture exceeded deadline")
    });
    (uri, rx, task)
}

fn source() -> Identity {
    Identity::from_hex(fixture::SOURCE).unwrap()
}
fn config(uri: &str, broker: &str) -> DbConnectionBuilder<RemoteModule> {
    bindings::DbConnection::builder().with_container_credentials(Container::new(fixture::SOURCE, uri, broker).unwrap())
}

async fn drive_until<M: SpacetimeModule>(session: &mut ContainerSession<M>, condition: impl Future<Output = ()>) {
    timeout(Duration::from_secs(5), async {
        tokio::select! {
            result = session.run() => panic!("session terminated unexpectedly: {result:?}"),
            _ = condition => {},
        }
    })
    .await
    .unwrap();
}

#[test]
fn rejects_ambiguous_initial_configuration_without_io() {
    let factory = |_, builder| builder;
    assert!(matches!(
        ContainerSession::new(bindings::DbConnection::builder(), factory),
        Err(ContainerSessionError::Configuration)
    ));
    let new = || config("https://must-not-contact.invalid", "http://127.0.0.1:1/v1/credentials");
    for builder in [
        new().with_token(Some("owner-secret")),
        new().with_debug_to_file("must-not-write"),
        new().on_connect(|_, _, _| {}),
    ] {
        let error = ContainerSession::new(builder, factory).err().unwrap();
        assert_eq!(error, ContainerSessionError::Configuration);
        assert!(!format!("{error:?} {error}").contains("owner-secret"));
    }
}

#[test]
fn requires_material_extension_of_both_returned_expiry_and_monotonic_deadline() {
    let old = Validity {
        expiry: UNIX_EPOCH + Duration::from_secs(100),
        deadline: Instant::now(),
    };
    assert!(!old.extended_by(Validity {
        expiry: old.expiry,
        deadline: old.deadline + Duration::from_secs(20)
    }));
    assert!(!old.extended_by(Validity {
        expiry: old.expiry + Duration::from_secs(20),
        deadline: old.deadline
    }));
    assert!(!old.extended_by(Validity {
        expiry: old.expiry + Duration::from_millis(999),
        deadline: old.deadline + Duration::from_secs(20)
    }));
    assert!(old.extended_by(Validity {
        expiry: old.expiry + Duration::from_secs(2),
        deadline: old.deadline + Duration::from_secs(2)
    }));
}

#[tokio::test]
async fn renewal_rebuilds_subscriptions_and_cache_without_replaying_calls() {
    let (broker, broker_task) = fixture::http_fixture(vec![
        fixture::response(200, &fixture::token_body("disposable-first", 3)),
        fixture::response(200, &fixture::token_body("disposable-second", 20)),
    ])
    .await;
    let broker_task = Task(Some(broker_task));
    let (uri, _wire_applied, peer_task) = peer(2, source()).await;
    let events = Arc::new(Mutex::new(Vec::new()));
    let event_capture = events.clone();
    let (ready, mut readiness) = mpsc::unbounded_channel();
    let drops = Arc::new(AtomicUsize::new(0));
    let captures = drops.clone();
    let mut session = ContainerSession::new(config(&uri, &broker), move |info, builder| {
        let ready = ready.clone();
        let captures = captures.clone();
        builder.on_connect(move |conn, _, _| {
            // The first generation will receive a row before rotation. The
            // second must still start with an empty, newly allocated cache.
            assert_eq!(conn.db.connected().count(), 0);
            conn.subscription_builder()
                .on_applied(move |ctx| {
                    assert_eq!(ctx.db.connected().count(), 1);
                    ready.send(info.generation).unwrap();
                })
                .subscribe("SELECT * FROM connected");
            if info.generation == 1 {
                struct Probe(Arc<AtomicUsize>);
                impl Drop for Probe {
                    fn drop(&mut self) {
                        self.0.fetch_add(1, Ordering::SeqCst);
                    }
                }
                let probe = Probe(captures);
                conn.reducers
                    .identity_connected_then(move |_, _| {
                        let _ = &probe;
                        panic!("unconfirmed reducer outcome must remain unknown");
                    })
                    .unwrap();
            }
        })
    })
    .unwrap()
    .on_event(move |event| event_capture.lock().unwrap().push(event));
    drive_until(&mut session, async {
        assert_eq!(readiness.recv().await, Some(1));
    })
    .await;
    let retained = session.current.as_ref().unwrap().context.clone();
    retained.invoke_procedure_with_callback::<_, ()>("unconfirmed", EmptyArgs {}, |_, _| {
        panic!("unconfirmed procedure must not report completion")
    });
    let first_aborts = session.current.as_ref().unwrap().aborts.clone();
    let first_cache = retained.cache.clone();
    drive_until(&mut session, async {
        assert_eq!(readiness.recv().await, Some(2));
    })
    .await;
    assert!(first_aborts.iter().all(AbortHandle::is_finished));
    assert!(!retained.is_active());
    assert!(!Arc::ptr_eq(
        &first_cache,
        &session.current.as_ref().unwrap().context.cache
    ));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    session.shutdown_and_join().await.unwrap();
    session.shutdown_and_join().await.unwrap();
    assert!(session.current.is_none());
    let requests = broker_task.join().await;
    assert_eq!(requests.len(), 2);
    assert!(requests
        .iter()
        .all(|request| request.ends_with(&format!("{{\"target_database\":\"{}\"}}", fixture::SOURCE))));
    let stats = peer_task.join().await;
    assert_eq!(stats[0].subscriptions, 1);
    assert_eq!(stats[0].reducers, 1);
    assert_eq!(stats[0].procedures, 1);
    assert_eq!(stats[1].subscriptions, 1);
    assert_eq!(stats[1].reducers, 0);
    assert_eq!(stats[1].procedures, 0);
    let events = events.lock().unwrap();
    let closed = events.iter().position(|event| matches!(event, ContainerSessionEvent::Closed { session, reason: ContainerSessionEndReason::CredentialRenewal, outstanding_calls: OutstandingCallOutcomes::Unknown } if session.generation == 1)).unwrap();
    let next = events
        .iter()
        .position(|event| matches!(event, ContainerSessionEvent::Connecting(info) if info.generation == 2))
        .unwrap();
    assert!(closed < next);
    assert!(!format!("{events:?}").contains("disposable-"));
}

#[tokio::test]
async fn unchanged_lease_expiry_does_not_rotate_and_denial_closes_the_active_session() {
    let body = fixture::token_body("disposable-same-lease", 3);
    let (broker, broker_task) = fixture::http_fixture(vec![
        fixture::response(200, &body),
        fixture::response(200, &body),
        fixture::response(200, &body),
        fixture::response(403, "private-error-body"),
    ])
    .await;
    let broker_task = Task(Some(broker_task));
    let (uri, _, peer_task) = peer(1, source()).await;
    let events = Arc::new(Mutex::new(Vec::new()));
    let capture = events.clone();
    let mut session = ContainerSession::new(config(&uri, &broker), |_, builder| builder)
        .unwrap()
        .on_event(move |event| capture.lock().unwrap().push(event));
    let error = timeout(Duration::from_secs(5), session.run())
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(
        error,
        ContainerSessionError::Credential(ContainerCredentialError::Denied)
    );
    assert!(session.current.is_none());
    assert_eq!(broker_task.join().await.len(), 4);
    assert_eq!(peer_task.join().await.len(), 1);
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| matches!(event, ContainerSessionEvent::Connecting(_)))
            .count(),
        1
    );
    assert!(!format!("{error:?} {:?}", events.lock().unwrap()).contains("private-error-body"));
    assert_eq!(session.run().await.unwrap_err(), error);
}

#[tokio::test]
async fn factory_cannot_override_the_pinned_target_or_supply_a_static_token() {
    let (broker, broker_task) = fixture::http_fixture(vec![fixture::response(
        200,
        &fixture::token_body("disposable-factory", 20),
    )])
    .await;
    let broker_task = Task(Some(broker_task));
    let mut session = ContainerSession::new(config("https://must-not-contact.invalid", &broker), |_, builder| {
        builder.with_database_name("other").with_token(Some("owner-secret"))
    })
    .unwrap();
    assert_eq!(session.run().await.unwrap_err(), ContainerSessionError::Configuration);
    assert_eq!(broker_task.join().await.len(), 1);
    assert!(session.current.is_none());
}

#[tokio::test]
async fn cancelling_token_request_preserves_resolved_target_and_closes_request_socket() {
    let server = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let uri = format!("http://{}", server.local_addr().unwrap());
    let resolver = Task::spawn(async move {
        let (mut socket, _) = server.accept().await.unwrap();
        assert!(fixture::read_request(&mut socket)
            .await
            .starts_with("GET /v1/database/alias/identity "));
        socket
            .write_all(&fixture::response(200, fixture::SOURCE))
            .await
            .unwrap();
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let broker = format!("http://{}/v1/credentials", listener.local_addr().unwrap());
    let (received, receipt) = oneshot::channel();
    let server = Task::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        fixture::read_request(&mut socket).await;
        received.send(()).unwrap();
        let mut byte = [0];
        assert_eq!(
            timeout(Duration::from_secs(2), socket.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        let (mut socket, _) = listener.accept().await.unwrap();
        assert!(fixture::read_request(&mut socket).await.contains(fixture::SOURCE));
        socket.write_all(&fixture::response(403, "denied")).await.unwrap();
    });
    let mut session =
        ContainerSession::new(config(&uri, &broker).with_database_name("alias"), |_, builder| builder).unwrap();
    drive_until(&mut session, async {
        receipt.await.unwrap();
    })
    .await;
    assert_eq!(session.target.as_ref().unwrap().1, source());
    resolver.join().await;
    // Resolver is gone. Resume still requests exactly the pinned Identity.
    assert_eq!(
        session.run().await.unwrap_err(),
        ContainerSessionError::Credential(ContainerCredentialError::Denied)
    );
    server.join().await;
    session.shutdown_and_join().await.unwrap_err();
}

#[tokio::test]
async fn dropping_owner_aborts_native_tasks_despite_retained_connection() {
    let (broker, broker_task) = fixture::http_fixture(vec![fixture::response(
        200,
        &fixture::token_body("disposable-drop", 20),
    )])
    .await;
    let broker_task = Task(Some(broker_task));
    let (uri, _, peer_task) = peer(1, source()).await;
    let (connected, receipt) = oneshot::channel();
    let mut connected = Some(connected);
    let mut session = ContainerSession::new(config(&uri, &broker), move |_, builder| {
        let connected = connected.take().unwrap();
        builder.on_connect(move |_, _, _| {
            connected.send(()).unwrap();
        })
    })
    .unwrap();
    drive_until(&mut session, async {
        receipt.await.unwrap();
    })
    .await;
    let retained = session.current.as_ref().unwrap().context.clone();
    let tasks = retained.inner.lock().unwrap().background_tasks.clone();
    drop(session);
    assert!(!retained.is_active());
    tasks.lock().await.stop_and_join().await.unwrap();
    broker_task.join().await;
    peer_task.join().await;
}

#[tokio::test]
async fn cancelling_shutdown_keeps_native_join_ownership_until_resumed() {
    // A task destructor deliberately waits on a gate. This proves that the
    // shutdown method retains the JoinHandle while its wait is cancelled.
    let (entered, entry) = oneshot::channel();
    let (release, gate) = std::sync::mpsc::channel();
    struct BlockingDrop {
        entered: Option<oneshot::Sender<()>>,
        gate: std::sync::mpsc::Receiver<()>,
    }
    impl Drop for BlockingDrop {
        fn drop(&mut self) {
            let _ = self.entered.take().unwrap().send(());
            self.gate.recv_timeout(Duration::from_secs(3)).unwrap();
        }
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let destructor = BlockingDrop {
        entered: Some(entered),
        gate,
    };
    let websocket = runtime.spawn(async move {
        let _destructor = destructor;
        futures::future::pending::<()>().await;
    });
    let parser = runtime.spawn(futures::future::pending());
    let mut session = ContainerSession::new(
        config("http://127.0.0.1:1", "http://127.0.0.1:1/v1/credentials"),
        |_, builder| builder,
    )
    .unwrap();
    let inner =
        super::super::build_db_ctx_inner::<RemoteModule>(None, NativeTasks::new(websocket, parser), None, None, None);
    let aborts = inner
        .lock()
        .unwrap()
        .background_tasks
        .try_lock()
        .unwrap()
        .abort_handles();
    let (outgoing, _outgoing_rx) = futures_channel::mpsc::unbounded();
    let (_incoming, incoming_rx) = futures_channel::mpsc::unbounded();
    let (pending, pending_rx) = futures_channel::mpsc::unbounded();
    let context = super::super::build_db_ctx(
        tokio::runtime::Handle::current(),
        inner,
        outgoing,
        Arc::new(tokio::sync::Mutex::new(incoming_rx)),
        pending,
        Arc::new(tokio::sync::Mutex::new(pending_rx)),
        None,
        None,
    );
    session.current = Some(Current {
        context,
        info: ContainerSessionInfo {
            generation: 1,
            target: source(),
        },
        validity: Validity {
            expiry: SystemTime::now(),
            deadline: Instant::now(),
        },
        refresh_at: Instant::now(),
        signal: Arc::new(Signal::default()),
        connected_announced: false,
        closing: None,
        driver_finished: false,
        aborts,
    });
    {
        let mut shutdown = Box::pin(session.shutdown_and_join());
        tokio::select! { result = &mut shutdown => panic!("shutdown completed before destructor release: {result:?}"), result = entry => result.unwrap(), }
    }
    assert!(session.current.is_some());
    release.send(()).unwrap();
    session.shutdown_and_join().await.unwrap();
    assert!(session.current.is_none());
    session.shutdown_and_join().await.unwrap();
    runtime.shutdown_background();
}

#[tokio::test]
async fn callback_panic_propagates_only_after_socket_and_native_tasks_close() {
    let (broker, broker_task) = fixture::http_fixture(vec![fixture::response(
        200,
        &fixture::token_body("disposable-panic", 20),
    )])
    .await;
    let broker_task = Task(Some(broker_task));
    let (uri, _, peer_task) = peer(1, source()).await;
    let mut session = ContainerSession::new(config(&uri, &broker), |_, builder| {
        builder.on_connect(|_, _, _| panic!("injected application panic"))
    })
    .unwrap();
    assert!(AssertUnwindSafe(session.run()).catch_unwind().await.is_err());
    assert!(session.current.is_none());
    assert!(session.stopped);
    broker_task.join().await;
    peer_task.join().await;
}

#[tokio::test]
async fn observer_panic_during_rotation_still_joins_the_previous_connection() {
    let (broker, broker_task) = fixture::http_fixture(vec![
        fixture::response(200, &fixture::token_body("disposable-old", 3)),
        fixture::response(200, &fixture::token_body("disposable-new", 20)),
    ])
    .await;
    let broker_task = Task(Some(broker_task));
    let (uri, _, peer_task) = peer(1, source()).await;
    let mut session = ContainerSession::new(config(&uri, &broker), |_, builder| builder)
        .unwrap()
        .on_event(|event| {
            if matches!(event, ContainerSessionEvent::Closing { .. }) {
                panic!("injected observer panic");
            }
        });
    assert!(AssertUnwindSafe(session.run()).catch_unwind().await.is_err());
    assert!(session.current.is_none());
    broker_task.join().await;
    peer_task.join().await;
}

#[tokio::test]
async fn unexpected_sender_is_not_exposed_to_application_callback() {
    let (broker, broker_task) = fixture::http_fixture(vec![fixture::response(
        200,
        &fixture::token_body("disposable-identity", 20),
    )])
    .await;
    let broker_task = Task(Some(broker_task));
    let (uri, _, peer_task) = peer(1, Identity::ZERO).await;
    let mut session = ContainerSession::new(config(&uri, &broker), |_, builder| {
        builder.on_connect(|_, _, _| panic!("wrong sender must never reach application"))
    })
    .unwrap();
    assert_eq!(
        session.run().await.unwrap_err(),
        ContainerSessionError::IdentityMismatch
    );
    assert!(session.current.is_none());
    broker_task.join().await;
    peer_task.join().await;
}

#[tokio::test]
async fn stalled_refresh_is_cancelled_at_expiry_and_never_extends_the_old_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let broker = format!("http://{}/v1/credentials", listener.local_addr().unwrap());
    let broker_task = Task::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        fixture::read_request(&mut socket).await;
        socket
            .write_all(&fixture::response(200, &fixture::token_body("disposable-expiring", 2)))
            .await
            .unwrap();
        drop(socket);
        let (mut socket, _) = listener.accept().await.unwrap();
        fixture::read_request(&mut socket).await;
        let mut byte = [0];
        assert_eq!(
            timeout(Duration::from_secs(3), socket.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    });
    let (uri, _, peer_task) = peer(1, source()).await;
    let (closed, receipt) = oneshot::channel();
    let mut closed = Some(closed);
    let mut session = ContainerSession::new(config(&uri, &broker), |_, builder| builder)
        .unwrap()
        .on_event(move |event| {
            if matches!(
                event,
                ContainerSessionEvent::Closing {
                    reason: ContainerSessionEndReason::CredentialExpiry,
                    ..
                }
            ) {
                closed.take().unwrap().send(()).unwrap();
            }
        });
    drive_until(&mut session, async {
        receipt.await.unwrap();
    })
    .await;
    if let Some(current) = &session.current {
        assert!(!current.context.is_active());
    }
    session.shutdown_and_join().await.unwrap();
    assert!(session.current.is_none());
    broker_task.join().await;
    peer_task.join().await;
}

#[tokio::test]
async fn factory_panic_leaves_no_connection_and_is_a_terminal_owner_failure() {
    let (broker, broker_task) = fixture::http_fixture(vec![fixture::response(
        200,
        &fixture::token_body("disposable-factory-panic", 20),
    )])
    .await;
    let broker_task = Task(Some(broker_task));
    let mut session = ContainerSession::new(config("https://must-not-contact.invalid", &broker), |_, _| {
        panic!("injected factory panic")
    })
    .unwrap();
    assert!(AssertUnwindSafe(session.run()).catch_unwind().await.is_err());
    assert_eq!(session.run().await.unwrap_err(), ContainerSessionError::Panicked);
    assert!(session.current.is_none());
    broker_task.join().await;
}

#[tokio::test]
async fn unexpected_sender_still_fails_when_initial_connection_is_followed_by_immediate_close() {
    let (broker, broker_task) = fixture::http_fixture(vec![fixture::response(
        200,
        &fixture::token_body("disposable-fast-close", 20),
    )])
    .await;
    let broker_task = Task(Some(broker_task));
    let (uri, _, peer_task) = peer_with_close(1, Identity::ZERO, true).await;
    let mut session = ContainerSession::new(config(&uri, &broker), |_, builder| {
        builder.on_connect(|_, _, _| panic!("wrong sender must never reach application"))
    })
    .unwrap();
    assert_eq!(
        timeout(Duration::from_secs(3), session.run())
            .await
            .unwrap()
            .unwrap_err(),
        ContainerSessionError::IdentityMismatch
    );
    assert!(session.current.is_none());
    broker_task.join().await;
    peer_task.join().await;
}
