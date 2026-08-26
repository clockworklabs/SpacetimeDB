//! Tests for connection replacement, the server side of SDK auto-reconnect.
//!
//! A reconnecting client supplies a stable `session_id`. When it reconnects
//! before the server has noticed the old socket died, the new connection
//! supersedes the old one. The old connection is torn down through the normal
//! disconnect sequence before the new connection's `client_connected` runs.

use anyhow::{bail, Context, Result};
use futures::{SinkExt, StreamExt};
use spacetimedb_client_api_messages::websocket::{common as ws_common, v2 as ws_v2, v3 as ws_v3};
use spacetimedb_lib::bsatn;
use spacetimedb_smoketests::Smoketest;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// A raw v3 websocket connection to a database, bypassing the SDKs so that a
/// test controls exactly which query parameters are sent.
struct TestConnection {
    socket: Socket,
    connection_id: String,
}

impl TestConnection {
    /// Open a connection, optionally supplying a `session_id`, and wait for the
    /// server's `InitialConnection` message.
    async fn open(test: &Smoketest, connection_id: &str, session_id: Option<&str>) -> Result<Self> {
        let token = test.read_token()?;
        let host = test.server_host();
        let database = test
            .database_identity
            .as_deref()
            .context("test database has not been published")?;

        // Uncompressed, so the test can decode payloads with plain BSATN.
        let mut url =
            format!("ws://{host}/v1/database/{database}/subscribe?compression=None&connection_id={connection_id}");
        if let Some(session_id) = session_id {
            url.push_str(&format!("&session_id={session_id}"));
        }

        let mut request = url.into_client_request()?;
        request
            .headers_mut()
            .insert(SEC_WEBSOCKET_PROTOCOL, ws_v3::BIN_PROTOCOL.parse()?);
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {token}").parse()?);

        let (socket, response) = connect_async(request).await?;
        let negotiated = response
            .headers()
            .get(SEC_WEBSOCKET_PROTOCOL)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if negotiated != ws_v3::BIN_PROTOCOL {
            bail!("server negotiated {negotiated:?}, expected {}", ws_v3::BIN_PROTOCOL);
        }

        let mut connection = Self {
            socket,
            connection_id: connection_id.to_string(),
        };
        match connection.next_message().await? {
            ws_v2::ServerMessage::InitialConnection(initial) => {
                let established = initial.connection_id.to_hex().to_string();
                if established != connection.connection_id {
                    bail!(
                        "server established connection id {established}, expected {}",
                        connection.connection_id
                    );
                }
            }
            other => bail!("expected InitialConnection, got {other:?}"),
        }
        Ok(connection)
    }

    /// Read the next server message, decoding the v3 framing, which packs one
    /// or more messages into a single binary payload.
    async fn next_message(&mut self) -> Result<ws_v2::ServerMessage> {
        loop {
            let message = self
                .socket
                .next()
                .await
                .context("websocket closed while awaiting a message")??;
            match message {
                Message::Binary(payload) => {
                    // Binary payloads start with a compression tag; the rest is
                    // one or more BSATN server messages back to back.
                    let (tag, mut body) = payload.split_first().context("empty binary websocket payload")?;
                    if *tag != ws_common::SERVER_MSG_COMPRESSION_TAG_NONE {
                        bail!("expected an uncompressed payload, got compression tag {tag}");
                    }
                    return Ok(bsatn::from_reader(&mut body)?);
                }
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(frame) => bail!("websocket closed: {frame:?}"),
                other => bail!("unexpected websocket message: {other:?}"),
            }
        }
    }

    async fn send(&mut self, message: ws_v2::ClientMessage) -> Result<()> {
        let payload = bsatn::to_vec(&message)?;
        self.socket.send(Message::Binary(payload.into())).await?;
        Ok(())
    }

    /// Whether the server still serves this connection.
    ///
    /// A superseded connection's actor is stopped, so a request on it is never
    /// answered. Note the server does not send a close frame. The peer's
    /// socket stays half-open until it writes, which is what this does.
    async fn is_still_served(&mut self) -> bool {
        if self
            .send(ws_v2::ClientMessage::Subscribe(ws_v2::Subscribe {
                request_id: 999,
                query_set_id: ws_v2::QuerySetId::new(999),
                query_strings: vec!["SELECT * FROM st_client".into()].into_boxed_slice(),
            }))
            .await
            .is_err()
        {
            return false;
        }
        matches!(
            tokio::time::timeout(std::time::Duration::from_secs(10), self.next_message()).await,
            Ok(Ok(_))
        )
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
}

/// The order of lifecycle log lines for the given connection ids.
fn lifecycle_log(test: &Smoketest) -> Vec<String> {
    test.logs(200)
        .unwrap_or_default()
        .into_iter()
        .filter(|line| line.contains("connected ") || line.contains("disconnected "))
        .collect()
}

fn position_of(lines: &[String], event: &str, connection_id: &str) -> Option<usize> {
    lines
        .iter()
        .position(|line| line.contains(&format!("{event} {connection_id}")))
}

/// Wait for a log line to appear, since the lifecycle reducers run
/// asynchronously with respect to the websocket handshake.
fn wait_for_log(test: &Smoketest, event: &str, connection_id: &str) -> Vec<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let lines = lifecycle_log(test);
        if position_of(&lines, event, connection_id).is_some() {
            return lines;
        }
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for `{event} {connection_id}` in logs: {lines:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

const CONNECTION_A: &str = "00000000000000000000000000000a11";
const CONNECTION_B: &str = "00000000000000000000000000000b22";
const CONNECTION_C: &str = "00000000000000000000000000000c33";
const SESSION: &str = "0000000000000000000000000000dead";
const OTHER_SESSION: &str = "0000000000000000000000000000beef";

/// A second connection with the same session id supersedes the first: the old
/// connection is disconnected, and its `client_disconnected` runs strictly
/// before the new connection's `client_connected`.
#[test]
fn test_reconnect_with_same_session_replaces_connection() {
    let test = Smoketest::builder().precompiled_module("connection-session").build();

    runtime().block_on(async {
        let mut first = TestConnection::open(&test, CONNECTION_A, Some(SESSION))
            .await
            .expect("first connection failed");
        wait_for_log(&test, "connected", CONNECTION_A);

        // Reconnect with the same session before the server notices the drop.
        let _second = TestConnection::open(&test, CONNECTION_B, Some(SESSION))
            .await
            .expect("second connection failed");

        assert!(
            !first.is_still_served().await,
            "the superseded connection should no longer be served"
        );

        let lines = wait_for_log(&test, "connected", CONNECTION_B);
        let connected_a = position_of(&lines, "connected", CONNECTION_A).expect("A never connected");
        let disconnected_a = position_of(&lines, "disconnected", CONNECTION_A).expect("A never disconnected");
        let connected_b = position_of(&lines, "connected", CONNECTION_B).expect("B never connected");

        assert!(
            connected_a < disconnected_a,
            "expected A to connect before disconnecting: {lines:?}"
        );
        assert!(
            disconnected_a < connected_b,
            "expected A's client_disconnected to run before B's client_connected: {lines:?}"
        );
    });
}

/// A connection with a different session id does not supersede: both stay live.
#[test]
fn test_different_session_does_not_replace_connection() {
    let test = Smoketest::builder().precompiled_module("connection-session").build();

    runtime().block_on(async {
        let _first = TestConnection::open(&test, CONNECTION_A, Some(SESSION))
            .await
            .expect("first connection failed");
        wait_for_log(&test, "connected", CONNECTION_A);

        let _second = TestConnection::open(&test, CONNECTION_B, Some(OTHER_SESSION))
            .await
            .expect("second connection failed");
        let lines = wait_for_log(&test, "connected", CONNECTION_B);

        assert!(
            position_of(&lines, "disconnected", CONNECTION_A).is_none(),
            "the first connection should still be live: {lines:?}"
        );
    });
}

/// A connection which supplies no session id behaves exactly as before: it
/// neither supersedes nor is superseded.
#[test]
fn test_connection_without_session_is_not_replaced() {
    let test = Smoketest::builder().precompiled_module("connection-session").build();

    runtime().block_on(async {
        let _first = TestConnection::open(&test, CONNECTION_A, None)
            .await
            .expect("first connection failed");
        wait_for_log(&test, "connected", CONNECTION_A);

        let _second = TestConnection::open(&test, CONNECTION_B, Some(SESSION))
            .await
            .expect("second connection failed");
        let lines = wait_for_log(&test, "connected", CONNECTION_B);

        assert!(
            position_of(&lines, "disconnected", CONNECTION_A).is_none(),
            "a connection without a session id should not be superseded: {lines:?}"
        );
    });
}

/// Repeated reconnects each supersede only the connection immediately before
/// them, leaving exactly one live connection for the session.
#[test]
fn test_repeated_reconnects_leave_one_live_connection() {
    let test = Smoketest::builder().precompiled_module("connection-session").build();

    runtime().block_on(async {
        let mut first = TestConnection::open(&test, CONNECTION_A, Some(SESSION))
            .await
            .expect("first connection failed");
        wait_for_log(&test, "connected", CONNECTION_A);

        let mut second = TestConnection::open(&test, CONNECTION_B, Some(SESSION))
            .await
            .expect("second connection failed");
        wait_for_log(&test, "connected", CONNECTION_B);
        assert!(!first.is_still_served().await, "A should have been superseded");

        let mut third = TestConnection::open(&test, CONNECTION_C, Some(SESSION))
            .await
            .expect("third connection failed");
        wait_for_log(&test, "connected", CONNECTION_C);
        assert!(!second.is_still_served().await, "B should have been superseded");

        let lines = lifecycle_log(&test);
        assert!(
            position_of(&lines, "disconnected", CONNECTION_B).is_some(),
            "B should have been superseded by C: {lines:?}"
        );
        assert!(
            position_of(&lines, "disconnected", CONNECTION_C).is_none(),
            "C should still be live: {lines:?}"
        );

        assert!(
            third.is_still_served().await,
            "the newest connection should still be served"
        );

        // Exactly one websocket client row remains for the session. The SQL
        // query itself opens a short-lived connection, so allow for one extra.
        let sql_out = test.sql("SELECT * FROM st_client").unwrap();
        let row_count = sql_out.lines().filter(|line| line.contains("0x")).count();
        assert!(
            row_count <= 2,
            "expected at most 2 st_client rows (the live connection and the SQL query's own), got {row_count}: {sql_out}"
        );
    });
}

/// A batch subscribe registers every query set atomically and answers with one
/// result per set, in request order.
#[test]
fn test_batch_subscribe_applies_all_sets() {
    let test = Smoketest::builder().precompiled_module("connection-session").build();

    runtime().block_on(async {
        let mut connection = TestConnection::open(&test, CONNECTION_A, Some(SESSION))
            .await
            .expect("connection failed");

        connection
            .send(ws_v2::ClientMessage::SubscribeBatch(ws_v2::SubscribeBatch {
                request_id: 1,
                sets: vec![
                    ws_v2::SubscribeSet {
                        query_set_id: ws_v2::QuerySetId::new(1),
                        query_strings: vec!["SELECT * FROM st_client".into()].into_boxed_slice(),
                    },
                    ws_v2::SubscribeSet {
                        query_set_id: ws_v2::QuerySetId::new(2),
                        query_strings: vec!["SELECT * FROM st_table".into()].into_boxed_slice(),
                    },
                ]
                .into_boxed_slice(),
            }))
            .await
            .expect("failed to send SubscribeBatch");

        match connection.next_message().await.expect("no response") {
            ws_v2::ServerMessage::SubscribeBatchApplied(applied) => {
                assert_eq!(applied.request_id, 1);
                assert_eq!(applied.results.len(), 2, "expected one result per set");
                assert_eq!(applied.results[0].query_set_id, ws_v2::QuerySetId::new(1));
                assert_eq!(applied.results[1].query_set_id, ws_v2::QuerySetId::new(2));
                for result in applied.results.iter() {
                    assert!(
                        matches!(result.outcome, ws_v2::SubscribeSetOutcome::Applied(_)),
                        "expected every set to apply, got {:?}",
                        result.outcome
                    );
                }
            }
            other => panic!("expected SubscribeBatchApplied, got {other:?}"),
        }
    });
}

/// A batch subscribe with one invalid query reports that set's error while the
/// other sets still apply.
#[test]
fn test_batch_subscribe_reports_per_set_errors() {
    let test = Smoketest::builder().precompiled_module("connection-session").build();

    runtime().block_on(async {
        let mut connection = TestConnection::open(&test, CONNECTION_A, Some(SESSION))
            .await
            .expect("connection failed");

        connection
            .send(ws_v2::ClientMessage::SubscribeBatch(ws_v2::SubscribeBatch {
                request_id: 7,
                sets: vec![
                    ws_v2::SubscribeSet {
                        query_set_id: ws_v2::QuerySetId::new(1),
                        query_strings: vec!["SELECT * FROM st_client".into()].into_boxed_slice(),
                    },
                    ws_v2::SubscribeSet {
                        query_set_id: ws_v2::QuerySetId::new(2),
                        query_strings: vec!["SELECT * FROM no_such_table".into()].into_boxed_slice(),
                    },
                ]
                .into_boxed_slice(),
            }))
            .await
            .expect("failed to send SubscribeBatch");

        match connection.next_message().await.expect("no response") {
            ws_v2::ServerMessage::SubscribeBatchApplied(applied) => {
                assert_eq!(applied.request_id, 7);
                assert!(
                    matches!(applied.results[0].outcome, ws_v2::SubscribeSetOutcome::Applied(_)),
                    "the valid set should apply, got {:?}",
                    applied.results[0].outcome
                );
                assert!(
                    matches!(applied.results[1].outcome, ws_v2::SubscribeSetOutcome::Error(_)),
                    "the invalid set should report an error, got {:?}",
                    applied.results[1].outcome
                );
            }
            other => panic!("expected SubscribeBatchApplied, got {other:?}"),
        }
    });
}
