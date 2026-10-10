#![cfg(not(target_arch = "wasm32"))]

#[path = "../examples/reconnect-desktop/src/module_bindings/mod.rs"]
mod module_bindings;

use module_bindings::*;
use spacetimedb_client_api_messages::websocket::v2 as ws;
use spacetimedb_lib::bsatn;
use spacetimedb_sdk::{AutomaticReconnectOptions, ConnectionId, DbContext, Identity};
use std::{
    net::TcpListener,
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_tungstenite::tungstenite::{
    self,
    handshake::server::{Request, Response},
    Message,
};

fn accept(stream: std::net::TcpStream) -> tungstenite::WebSocket<std::net::TcpStream> {
    tungstenite::accept_hdr(stream, |_: &Request, mut response: Response| {
        response
            .headers_mut()
            .insert("sec-websocket-protocol", "v2.bsatn.spacetimedb".parse().unwrap());
        Ok(response)
    })
    .unwrap()
}

fn send(socket: &mut tungstenite::WebSocket<std::net::TcpStream>, message: ws::ServerMessage) {
    let mut bytes = vec![0]; // uncompressed binary protocol
    bytes.extend(bsatn::to_vec(&message).unwrap());
    socket.send(Message::Binary(bytes.into())).unwrap();
}
fn tick_until(connection: &DbConnection, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !ready() {
        let _ = connection.frame_tick();
        assert!(Instant::now() < deadline, "connection did not reach expected state");
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn policy() -> AutomaticReconnectOptions {
    AutomaticReconnectOptions {
        min_delay: Duration::from_millis(500),
        max_delay: Duration::from_secs(1),
    }
}

#[test]
fn reconnect_preserves_session_and_credentials_and_stops_on_identity_change() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let (drop_tx, drop_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        for round in 0..3 {
            let stream = listener.accept().unwrap().0;
            let mut socket = tungstenite::accept_hdr(stream, |request: &Request, mut response: Response| {
                captured.lock().unwrap().push((
                    request.uri().to_string(),
                    request.headers().get("authorization").cloned(),
                ));
                response
                    .headers_mut()
                    .insert("sec-websocket-protocol", "v2.bsatn.spacetimedb".parse().unwrap());
                Ok(response)
            })
            .unwrap();
            send(
                &mut socket,
                ws::ServerMessage::InitialConnection(ws::InitialConnection {
                    identity: Identity::from_byte_array([if round == 2 { 2 } else { 1 }; 32]),
                    token: "retained-token".into(),
                    connection_id: ConnectionId::from_u128(round + 1),
                }),
            );
            drop_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        }
    });
    let events = Arc::new(Mutex::new(Vec::new()));
    let connect = events.clone();
    let reconnect = events.clone();
    let disconnect = events.clone();
    let error = events.clone();
    let connection = DbConnection::builder()
        .with_uri(format!("http://{address}"))
        .with_database_name("test")
        .with_automatic_reconnect_options(policy())
        .on_connect(move |_, _, _| connect.lock().unwrap().push("connect"))
        .on_automatic_reconnect(move |_, _, _| reconnect.lock().unwrap().push("reconnect"))
        .on_disconnect(move |_, _, next| {
            assert_eq!(next.unwrap().attempt, 1);
            disconnect.lock().unwrap().push("disconnect");
        })
        .on_connect_error(move |_, _, next| {
            assert!(next.is_none());
            error.lock().unwrap().push("terminal");
        })
        .build()
        .unwrap();
    tick_until(&connection, || events.lock().unwrap().len() == 1);
    let identity = connection.identity();
    drop_tx.send(()).unwrap();
    tick_until(&connection, || events.lock().unwrap().len() == 3);
    assert_eq!(connection.identity(), identity);
    assert_eq!(connection.connection_id(), ConnectionId::from_u128(2));
    drop_tx.send(()).unwrap();
    tick_until(&connection, || events.lock().unwrap().len() == 5);
    assert!(!connection.is_active() && !connection.is_reconnecting());
    assert_eq!(
        *events.lock().unwrap(),
        ["connect", "disconnect", "reconnect", "disconnect", "terminal"]
    );
    let requests = requests.lock().unwrap();
    let session = |uri: &str| {
        uri.split("session_id=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_owned()
    };
    assert_eq!(session(&requests[0].0), session(&requests[1].0));
    assert!(requests[0].1.is_none());
    assert_eq!(requests[1].1.as_ref().unwrap(), "Bearer retained-token");
    drop_tx.send(()).unwrap();
    server.join().unwrap();
}

#[test]
fn initial_handshake_failure_never_retries() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let socket = accept(listener.accept().unwrap().0);
        drop(socket); // No InitialConnection.
    });
    let errors = Arc::new(Mutex::new(0));
    let called = errors.clone();
    let connection = DbConnection::builder()
        .with_uri(format!("http://{address}"))
        .with_database_name("test")
        .with_automatic_reconnect()
        .on_connect_error(move |_, _, next| {
            assert!(next.is_none());
            *called.lock().unwrap() += 1;
        })
        .build()
        .unwrap();
    tick_until(&connection, || *errors.lock().unwrap() == 1);
    assert!(!connection.is_reconnecting());
    server.join().unwrap();
}

#[test]
fn in_flight_call_is_unknown_and_disconnect_cancels_backoff() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let mut socket = accept(listener.accept().unwrap().0);
        send(
            &mut socket,
            ws::ServerMessage::InitialConnection(ws::InitialConnection {
                identity: Identity::from_byte_array([1; 32]),
                token: "token".into(),
                connection_id: ConnectionId::from_u128(1),
            }),
        );
        // Accept a reducer but deliberately lose its result.
        let Message::Binary(bytes) = socket.read().unwrap() else {
            panic!("expected reducer");
        };
        assert!(matches!(
            bsatn::from_slice::<ws::ClientMessage>(&bytes).unwrap(),
            ws::ClientMessage::CallReducer(_)
        ));
    });
    let disconnects = Arc::new(Mutex::new(Vec::new()));
    let events = disconnects.clone();
    let connection = DbConnection::builder()
        .with_uri(format!("http://{address}"))
        .with_database_name("test")
        .with_automatic_reconnect_options(policy())
        .on_disconnect(move |_, error, next| {
            events.lock().unwrap().push((error.is_none(), next));
        })
        .build()
        .unwrap();
    tick_until(&connection, || connection.try_identity().is_some());
    let unknown = Arc::new(Mutex::new(false));
    let result = unknown.clone();
    connection
        .reducers
        .change_both_then(move |_, outcome| *result.lock().unwrap() = outcome.unwrap_err().is_unknown_result())
        .unwrap();
    tick_until(&connection, || connection.is_reconnecting());
    assert!(*unknown.lock().unwrap());
    assert!(matches!(
        connection.reducers.change_both(),
        Err(spacetimedb_sdk::Error::Disconnected)
    ));
    connection.disconnect().unwrap();
    assert!(connection.frame_tick().is_err());
    assert!(!connection.is_active() && !connection.is_reconnecting());
    assert_eq!(disconnects.lock().unwrap().len(), 2);
    assert!(disconnects.lock().unwrap()[1].0);
    assert!(disconnects.lock().unwrap()[1].1.is_none());
    server.join().unwrap();
}

#[test]
fn malformed_protocol_is_terminal() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (go, wait) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let mut socket = accept(listener.accept().unwrap().0);
        send(
            &mut socket,
            ws::ServerMessage::InitialConnection(ws::InitialConnection {
                identity: Identity::from_byte_array([1; 32]),
                token: "token".into(),
                connection_id: ConnectionId::from_u128(1),
            }),
        );
        wait.recv().unwrap();
        socket.send(Message::Binary(vec![0, 255].into())).unwrap();
    });
    let terminal = Arc::new(Mutex::new(false));
    let event = terminal.clone();
    let connection = DbConnection::builder()
        .with_uri(format!("http://{address}"))
        .with_database_name("test")
        .with_automatic_reconnect_options(policy())
        .on_disconnect(move |_, error, next| {
            assert!(error.is_some());
            assert!(next.is_none());
            *event.lock().unwrap() = true;
        })
        .build()
        .unwrap();
    tick_until(&connection, || connection.try_identity().is_some());
    go.send(()).unwrap();
    tick_until(&connection, || *terminal.lock().unwrap());
    assert!(!connection.is_reconnecting());
    server.join().unwrap();
}

#[test]
fn rejected_retained_token_forces_refresh_even_after_provider_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (go, wait) = mpsc::channel();
    let server = std::thread::spawn(move || {
        for round in 0..3 {
            let stream = listener.accept().unwrap().0;
            let socket = tungstenite::accept_hdr(stream, |request: &Request, mut response: Response| {
                if round == 1 {
                    assert!(request.headers()["authorization"]
                        .to_str()
                        .unwrap()
                        .starts_with("Bearer header."));
                    Err(tungstenite::http::Response::builder()
                        .status(401)
                        .body(Some("Rejected retained token".into()))
                        .unwrap())
                } else {
                    if round == 2 {
                        assert_eq!(request.headers()["authorization"], "Bearer refreshed");
                    }
                    response
                        .headers_mut()
                        .insert("sec-websocket-protocol", "v2.bsatn.spacetimedb".parse().unwrap());
                    Ok(response)
                }
            });
            if round == 1 {
                assert!(socket.is_err());
                continue;
            }
            let mut socket = socket.unwrap();
            send(
                &mut socket,
                ws::ServerMessage::InitialConnection(ws::InitialConnection {
                    identity: Identity::from_byte_array([1; 32]),
                    // Valid far-future exp: refresh must be forced by the rejection.
                    token: "header.eyJleHAiOjQwMDAwMDAwMDAsImlhdCI6MTcwMDAwMDAwMH0.signature".into(),
                    connection_id: ConnectionId::from_u128(round + 1),
                }),
            );
            wait.recv_timeout(Duration::from_secs(10)).unwrap();
        }
    });
    let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let called = calls.clone();
    let reconnects = Arc::new(Mutex::new(0));
    let reconnected = reconnects.clone();
    let connection = DbConnection::builder()
        .with_uri(format!("http://{address}"))
        .with_database_name("test")
        .with_automatic_reconnect_options(policy())
        .with_token_provider(move || {
            let attempt = called.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async move {
                if attempt == 0 {
                    Err(spacetimedb_sdk::Error::TokenProvider {
                        message: "refresh unavailable".into(),
                    })
                } else {
                    Ok("refreshed".into())
                }
            }
        })
        .on_automatic_reconnect(move |_, _, _| *reconnected.lock().unwrap() += 1)
        .build()
        .unwrap();
    tick_until(&connection, || connection.try_identity().is_some());
    go.send(()).unwrap();
    tick_until(&connection, || *reconnects.lock().unwrap() == 1);
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    connection.disconnect().unwrap();
    let _ = connection.frame_tick();
    go.send(()).unwrap();
    server.join().unwrap();
}

#[test]
fn async_runner_survives_a_drop_and_stops_from_reconnect_callback() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for round in 0..4 {
            let mut socket = accept(listener.accept().unwrap().0);
            send(
                &mut socket,
                ws::ServerMessage::InitialConnection(ws::InitialConnection {
                    identity: Identity::from_byte_array([1; 32]),
                    token: "retained".into(),
                    connection_id: ConnectionId::from_u128(round + 1),
                }),
            );
            if round == 3 {
                let _ = socket.read();
            }
        }
    });
    let reconnected = Arc::new(Mutex::new(0));
    let called = reconnected.clone();
    let connection = DbConnection::builder()
        .with_uri(format!("http://{address}"))
        .with_database_name("test")
        .with_automatic_reconnect_options(policy())
        .on_automatic_reconnect(move |ctx, _, _| {
            let mut count = called.lock().unwrap();
            *count += 1;
            if *count == 3 {
                ctx.disconnect().unwrap();
            }
        })
        .build()
        .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(8), connection.run_async())
            .await
            .unwrap()
            .unwrap();
    });
    assert_eq!(*reconnected.lock().unwrap(), 3);
    server.join().unwrap();
}
