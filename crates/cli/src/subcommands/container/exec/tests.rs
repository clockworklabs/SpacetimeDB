use super::session::{Input, Io, OutputSender};
use super::*;
use futures::{SinkExt, StreamExt};
use reqwest::{header::HeaderValue, Url};
use spacetimedb_lib::{
    container::exec::{self as wire, ClientControl, ExecReady, ServerControl},
    Identity, Uuid,
};
use tokio::{
    net::TcpListener,
    sync::{mpsc, oneshot},
};
use tokio_tungstenite::tungstenite::{
    handshake::server::{Request, Response},
    Message,
};

fn arguments(values: &[&str]) -> Result<ArgMatches, clap::Error> {
    cli().try_get_matches_from(values)
}
#[test]
fn literal_arguments_and_environment_are_bounded_and_redacted() {
    cli().help_expected(true).debug_assert();
    let args = arguments(&[
        "exec",
        "db",
        "-i",
        "--workdir",
        "/a b",
        "-e",
        "UNICODE=🦀=literal",
        "--",
        "/bin/echo",
        "$(no-shell)",
        "--flag",
    ])
    .unwrap();
    let value = start(&args, 42).unwrap();
    assert_eq!(value.argv, ["/bin/echo", "$(no-shell)", "--flag"]);
    assert_eq!(value.environment["UNICODE"], "🦀=literal");
    assert_eq!(value.working_directory.as_deref(), Some("/a b"));
    for invalid in ["SPACETIMEDB_TOKEN=secret-value", "BAD-NAME=secret-value", "NO_EQUALS"] {
        let args = arguments(&["exec", "db", "-e", invalid, "--", "true"]).unwrap();
        let error = start(&args, 1).unwrap_err().to_string();
        assert!(!error.contains("secret-value"));
    }
    let args = arguments(&["exec", "db", "-e", "A=1", "-e", "A=2", "--", "true"]).unwrap();
    assert!(start(&args, 1).is_err());
    assert!(arguments(&["exec", "db", "-t", "--", "true"]).is_err());
    let args = arguments(&["exec", "db", "--workdir", "relative", "--", "true"]).unwrap();
    assert!(start(&args, 1).is_err());
}

#[tokio::test]
async fn real_loopback_socket_preserves_binary_eof_controls_and_exit() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!(
        "http://{}/v1/database/{}/container/exec",
        listener.local_addr().unwrap(),
        Identity::ZERO.to_hex()
    );
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_hdr_async(stream, |request: &Request, mut response: Response| {
            assert_eq!(request.uri().query(), Some("generation=42"));
            assert_eq!(request.headers()["authorization"], "Bearer owned-loopback-fixture");
            assert_eq!(request.headers()["sec-websocket-protocol"], wire::SUBPROTOCOL);
            assert!(!request.uri().to_string().contains("owned-loopback-fixture"));
            response
                .headers_mut()
                .insert("sec-websocket-protocol", HeaderValue::from_static(wire::SUBPROTOCOL));
            Ok(response)
        })
        .await
        .unwrap();
        let first = socket.next().await.unwrap().unwrap();
        let ClientControl::Start(start) = ClientControl::decode(first.into_data().as_ref()).unwrap() else {
            panic!("missing Start")
        };
        assert_eq!(start.argv, ["/bin/echo", "literal"]);
        let ready = ServerControl::Ready(ExecReady {
            database_identity: Identity::ZERO,
            generation: 42,
            session_id: Uuid::from_u128(7),
            tty: false,
        });
        socket
            .send(Message::Text(serde_json::to_string(&ready).unwrap().into()))
            .await
            .unwrap();
        let mut data = false;
        let mut eof = false;
        let mut signal = false;
        let mut resize = false;
        while !(data && eof && signal && resize) {
            match socket.next().await.unwrap().unwrap() {
                Message::Binary(value) => {
                    assert_eq!(wire::decode_stdin(&value).unwrap(), &[0, 255, 1]);
                    data = true;
                }
                Message::Text(value) => match ClientControl::decode(value.as_bytes()).unwrap() {
                    ClientControl::StdinEof => {
                        assert!(!eof);
                        eof = true;
                    }
                    ClientControl::Signal(10) => signal = true,
                    ClientControl::Resize(wire::TerminalSize { rows: 24, columns: 80 }) => resize = true,
                    _ => panic!("unexpected control"),
                },
                _ => panic!("unexpected frame"),
            }
        }
        for stream in [wire::Stream::Stdout, wire::Stream::Stderr] {
            socket
                .send(Message::Binary(
                    wire::encode_data(stream, &[255, 0, 128]).unwrap().into(),
                ))
                .await
                .unwrap();
        }
        socket
            .send(Message::Text(
                serde_json::to_string(&ServerControl::Exit { exit_code: 37 })
                    .unwrap()
                    .into(),
            ))
            .await
            .unwrap();
    });
    let (input_tx, input) = mpsc::channel(2);
    input_tx.send(Input::Data(vec![0, 255, 1])).await.unwrap();
    input_tx.send(Input::Eof).await.unwrap();
    let (output_tx, mut output) = mpsc::channel::<session::Output>(1);
    let (_completed, completion) = oneshot::channel();
    let mut io = Io {
        input,
        output: OutputSender {
            sender: output_tx,
            wake: None,
        },
        completion,
    };
    let options = start(
        &arguments(&["exec", "db", "-i", "--", "/bin/echo", "literal"]).unwrap(),
        42,
    )
    .unwrap();
    let signals = futures::stream::iter([
        Ok(ClientControl::Signal(10)),
        Ok(ClientControl::Resize(wire::TerminalSize { rows: 24, columns: 80 })),
    ])
    .chain(futures::stream::pending());
    let client = session::run(
        Url::parse(&origin).unwrap(),
        HeaderValue::from_static("Bearer owned-loopback-fixture"),
        Identity::ZERO,
        options,
        &mut io,
        signals,
    );
    let output_reader = async {
        let mut seen = Vec::new();
        for _ in 0..2 {
            let value = output.recv().await.unwrap();
            assert_eq!(value.bytes, [255, 0, 128]);
            seen.push(value.stream);
            value.completed.send(Ok(())).unwrap();
        }
        assert_eq!(seen, [wire::Stream::Stdout, wire::Stream::Stderr]);
    };
    let results = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(client, output_reader)
    })
    .await;
    if results.is_err() {
        server.abort();
    }
    let joined = server.await;
    assert!(joined.is_ok());
    assert_eq!(results.unwrap().0.unwrap(), 37);
}

#[test]
fn status_requires_the_exact_running_generation_without_http_readiness() {
    use spacetimedb_lib::container::operations::*;
    let mut status = ContainerStatus {
        database_identity: Identity::ZERO,
        published: true,
        endpoints: EndpointStatus::Pending,
        operational: Some(OperationalState {
            desired_revision: spacetimedb_lib::Hash::from_byte_array([0; 32]),
            desired_state: DesiredState::Running,
            generation: 42,
            condition: Condition::None,
            restart_pending: false,
            restart_attempt: 0,
            restart_not_before_ms: 0,
            current_instance: Some(CurrentInstance {
                generation: 42,
                state: ObservedState::Running,
                observed_revision: None,
                applied_env_generation: None,
                exit_code: None,
                oom_killed: false,
                condition: Condition::None,
            }),
        }),
    };
    assert_eq!(running_generation(&status).unwrap(), 42);
    for state in [
        ObservedState::Pending,
        ObservedState::Starting,
        ObservedState::Draining,
        ObservedState::Stopped,
        ObservedState::Completed,
        ObservedState::Failed,
    ] {
        status
            .operational
            .as_mut()
            .unwrap()
            .current_instance
            .as_mut()
            .unwrap()
            .state = state;
        assert!(running_generation(&status).is_err());
    }
    status
        .operational
        .as_mut()
        .unwrap()
        .current_instance
        .as_mut()
        .unwrap()
        .state = ObservedState::Ready;
    assert_eq!(running_generation(&status).unwrap(), 42);
    status
        .operational
        .as_mut()
        .unwrap()
        .current_instance
        .as_mut()
        .unwrap()
        .generation = 41;
    assert!(running_generation(&status).is_err());
    status
        .operational
        .as_mut()
        .unwrap()
        .current_instance
        .as_mut()
        .unwrap()
        .generation = 42;
    status.operational.as_mut().unwrap().desired_state = DesiredState::Stopped;
    assert!(running_generation(&status).is_err());
}

#[tokio::test]
async fn real_loopback_rejects_bad_bindings_order_and_unobserved_exit_without_replay() {
    for case in 0..8 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = Url::parse(&format!(
            "http://{}/v1/database/{}/container/exec",
            listener.local_addr().unwrap(),
            Identity::ZERO.to_hex()
        ))
        .unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async(stream, move |_: &Request, mut response: Response| {
                if case != 0 {
                    response
                        .headers_mut()
                        .insert("sec-websocket-protocol", HeaderValue::from_static(wire::SUBPROTOCOL));
                }
                Ok(response)
            })
            .await
            .unwrap();
            if case != 0 {
                let first = socket.next().await.unwrap().unwrap();
                assert!(matches!(
                    ClientControl::decode(first.into_data().as_ref()).unwrap(),
                    ClientControl::Start(_)
                ));
                let ready = ServerControl::Ready(ExecReady {
                    database_identity: if case == 1 {
                        Identity::from_byte_array([1; 32])
                    } else {
                        Identity::ZERO
                    },
                    generation: if case == 2 { 43 } else { 42 },
                    session_id: Uuid::from_u128(if case == 3 { 0 } else { 7 }),
                    tty: case == 4,
                });
                if case == 5 {
                    socket
                        .send(Message::Binary(
                            wire::encode_data(wire::Stream::Stdout, b"premature").unwrap().into(),
                        ))
                        .await
                        .unwrap();
                } else {
                    socket
                        .send(Message::Text(serde_json::to_string(&ready).unwrap().into()))
                        .await
                        .unwrap();
                    if case == 6 {
                        socket
                            .send(Message::Text(serde_json::to_string(&ready).unwrap().into()))
                            .await
                            .unwrap();
                    }
                    if case == 7 {
                        socket.close(None).await.unwrap();
                    }
                }
            }
            // The client must close this exact connection and never dial a retry.
            let closed = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next()).await;
            assert!(matches!(closed, Ok(None | Some(Err(_)) | Some(Ok(Message::Close(_))))));
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(30), listener.accept())
                    .await
                    .is_err()
            );
        });
        let (_input_tx, input) = mpsc::channel(1);
        let (output_tx, _output) = mpsc::channel(1);
        let (_completed, completion) = oneshot::channel();
        let mut io = Io {
            input,
            output: OutputSender {
                sender: output_tx,
                wake: None,
            },
            completion,
        };
        let options = start(&arguments(&["exec", "db", "--", "true"]).unwrap(), 42).unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(4),
            session::run(
                url,
                HeaderValue::from_static("Bearer owned-loopback-fixture"),
                Identity::ZERO,
                options,
                &mut io,
                futures::stream::pending(),
            ),
        )
        .await;
        if result.is_err() {
            server.abort();
        }
        let joined = server.await;
        assert!(joined.is_ok(), "case {case}");
        assert!(result.unwrap().is_err(), "case {case}");
    }
}

#[tokio::test]
async fn status_uses_authenticated_explicit_loopback_and_pins_the_resolved_identity() {
    use spacetimedb_lib::container::operations::EndpointStatus;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let identity = Identity::from_byte_array([7; 32]);
    let status = ContainerStatus {
        database_identity: identity,
        published: true,
        operational: None,
        endpoints: EndpointStatus::Pending,
    };
    let router = axum::Router::new().route(
        "/v1/database/owned-test-db/container/status",
        axum::routing::get(move |headers: axum::http::HeaderMap| async move {
            assert_eq!(headers["authorization"], "Bearer owned-loopback-fixture");
            axum::Json(status)
        }),
    );
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
    });
    let client = super::super::operations::ContainerClient::new(
        origin,
        HeaderValue::from_static("Bearer owned-loopback-fixture"),
    )
    .unwrap();
    let result = client.status("owned-test-db").await;
    let _ = stop.send(());
    let joined = server.await.unwrap();
    joined.unwrap();
    let status = result.unwrap();
    assert_eq!(status.database_identity, identity);
    let endpoint = client.url(status.database_identity.to_hex().as_ref(), "exec").unwrap();
    assert_eq!(
        endpoint.path(),
        format!("/v1/database/{}/container/exec", identity.to_hex())
    );
    assert!(running_generation(&status).is_err());
}
