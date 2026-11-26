//! End-to-end tests that the server answers protocol-level `Ping`s.
//!
//! The server answers each `Ping` as soon as it is read from the socket, in
//! `ws_answer_pings`, and the message handler ignores `Ping`s. So if that stage
//! were ever unwired, `Ping`s would get no reply at all, and these tests would
//! time out. They check lone and batched `Ping`s over a raw v3 websocket, and
//! that each `Pong` carries sensible values.

use anyhow::{bail, Context, Result};
use futures::{SinkExt, StreamExt};
use spacetimedb_client_api_messages::websocket::{common as ws_common, v2 as ws_v2, v3 as ws_v3};
use spacetimedb_lib::{bsatn, Timestamp};
use spacetimedb_smoketests::Smoketest;
use std::time::Duration;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

/// How long to wait for the server's replies to a request.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

fn reply_deadline() -> Instant {
    Instant::now() + REPLY_TIMEOUT
}

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// A raw, uncompressed v3 websocket connection, bypassing the SDKs so that a
/// test controls exactly how messages are framed.
struct TestConnection {
    socket: Socket,
}

impl TestConnection {
    /// Open a connection and wait for the server's `InitialConnection` message.
    async fn open(test: &Smoketest) -> Result<Self> {
        let token = test.read_token()?;
        let host = test.server_host();
        let database = test
            .database_identity
            .as_deref()
            .context("test database has not been published")?;

        // Uncompressed, so the test can decode payloads with plain BSATN.
        let mut request =
            format!("ws://{host}/v1/database/{database}/subscribe?compression=None").into_client_request()?;
        request
            .headers_mut()
            .insert(SEC_WEBSOCKET_PROTOCOL, ws_v3::BIN_PROTOCOL.parse()?);
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {token}").parse()?);

        let (socket, _) = connect_async(request).await?;
        let mut connection = Self { socket };
        match connection.next_messages(reply_deadline()).await?.as_slice() {
            [ws_v2::ServerMessage::InitialConnection(_)] => Ok(connection),
            other => bail!("expected InitialConnection, got {other:?}"),
        }
    }

    /// Send `messages` together in a single websocket message.
    async fn send(&mut self, messages: &[ws_v2::ClientMessage]) -> Result<()> {
        let mut payload = Vec::new();
        for message in messages {
            bsatn::to_writer(&mut payload, message)?;
        }
        self.socket.send(Message::Binary(payload.into())).await?;
        Ok(())
    }

    /// Read the server messages in the next websocket message.
    ///
    /// A v3 payload starts with a compression tag, followed by one or more
    /// BSATN server messages back to back.
    ///
    /// Fails if none arrives by `deadline`. The deadline is fixed by the caller,
    /// rather than restarted here for each frame, because the server's keepalive
    /// Ping frames would otherwise keep a wait for a missing reply going forever.
    async fn next_messages(&mut self, deadline: Instant) -> Result<Vec<ws_v2::ServerMessage>> {
        loop {
            let message = tokio::time::timeout_at(deadline, self.socket.next())
                .await
                .context("timed out awaiting a server message")?
                .context("websocket closed while awaiting a message")??;
            match message {
                Message::Binary(payload) => {
                    let (tag, mut body) = payload.split_first().context("empty binary websocket payload")?;
                    if *tag != ws_common::SERVER_MSG_COMPRESSION_TAG_NONE {
                        bail!("expected an uncompressed payload, got compression tag {tag}");
                    }
                    let mut messages = Vec::new();
                    while !body.is_empty() {
                        messages.push(bsatn::from_reader(&mut body)?);
                    }
                    return Ok(messages);
                }
                Message::Ping(_) | Message::Pong(_) => continue,
                other => bail!("unexpected websocket message: {other:?}"),
            }
        }
    }

    /// Read server messages until there is a `Pong` for every id in `pong_ids`
    /// and, if given, the `OneOffQueryResult` for `query_id`.
    ///
    /// Returns the `Pong`s in the order they arrived.
    async fn await_replies(&mut self, pong_ids: &[u32], query_id: Option<u32>) -> Result<Vec<ws_v2::Pong>> {
        let mut pongs = Vec::new();
        let mut query_answered = query_id.is_none();
        let deadline = reply_deadline();
        while pongs.len() < pong_ids.len() || !query_answered {
            for message in self.next_messages(deadline).await? {
                match message {
                    ws_v2::ServerMessage::Pong(pong) => pongs.push(pong),
                    ws_v2::ServerMessage::OneOffQueryResult(result) if Some(result.request_id) == query_id => {
                        query_answered = true
                    }
                    other => bail!("unexpected server message: {other:?}"),
                }
            }
        }
        Ok(pongs)
    }
}

fn query(request_id: u32) -> ws_v2::ClientMessage {
    ws_v2::ClientMessage::OneOffQuery(ws_v2::OneOffQuery {
        request_id,
        query_string: "SELECT * FROM st_table".into(),
    })
}

fn ping(request_id: u32) -> ws_v2::ClientMessage {
    ws_v2::ClientMessage::Ping(ws_v2::Ping {
        request_id,
        client_send_time: request_id.into(),
    })
}

/// The request ids of `pongs`, checking that each echoes its `Ping` and
/// reports a plausible receive time and hold duration.
fn pong_request_ids(pongs: &[ws_v2::Pong]) -> Vec<u32> {
    pongs
        .iter()
        .map(|pong| {
            assert_eq!(
                pong.client_send_time,
                u64::from(pong.request_id),
                "the pong should echo its ping's client_send_time: {pong:?}"
            );
            // The server runs on this machine, so it shares our clock.
            let now = Timestamp::now();
            let skew = now
                .duration_since(pong.server_receive_time)
                .or_else(|| pong.server_receive_time.duration_since(now))
                .unwrap();
            assert!(
                skew < Duration::from_secs(10),
                "server_receive_time is {skew:?} away from the local clock: {pong:?}"
            );
            assert!(
                pong.server_hold_duration.to_duration_abs() < Duration::from_secs(10),
                "implausible server_hold_duration: {pong:?}"
            );
            pong.request_id
        })
        .collect()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
}

/// A `Ping` sent in its own websocket message is answered.
#[test]
fn test_lone_ping_is_answered() {
    let test = Smoketest::builder().precompiled_module("noop").build();

    runtime().block_on(async {
        let mut connection = TestConnection::open(&test).await.expect("connection failed");
        connection.send(&[ping(1)]).await.unwrap();

        let pongs = connection.await_replies(&[1], None).await.unwrap();
        assert_eq!(pong_request_ids(&pongs), [1]);
    });
}

/// Every `Ping` in a v3 payload is answered, whether the payload holds only
/// `Ping`s or other messages too, and the other messages are still handled.
#[test]
fn test_batched_pings_are_answered() {
    let test = Smoketest::builder().precompiled_module("noop").build();

    runtime().block_on(async {
        let mut connection = TestConnection::open(&test).await.expect("connection failed");

        connection.send(&[ping(1), ping(2)]).await.unwrap();
        let mut ids = pong_request_ids(&connection.await_replies(&[1, 2], None).await.unwrap());
        ids.sort();
        assert_eq!(ids, [1, 2]);

        connection.send(&[query(3), ping(4), ping(5)]).await.unwrap();
        let mut ids = pong_request_ids(&connection.await_replies(&[4, 5], Some(3)).await.unwrap());
        ids.sort();
        assert_eq!(ids, [4, 5]);
    });
}
