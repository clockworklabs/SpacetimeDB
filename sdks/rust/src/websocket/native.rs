use super::shared::{
    decode_v2_server_message, encode_v2_client_message_bytes, make_uri_impl, UriError, WsConnection, WsError, WsParams,
};
use bytes::Bytes;
use futures::TryStreamExt;
use futures::{SinkExt, StreamExt as _};
use futures_channel::mpsc;
use http::uri::Uri;
use spacetimedb_client_api_messages::websocket as ws;
use spacetimedb_lib::{bsatn, ConnectionId};
use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::mem;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::{net::TcpStream, runtime, task::JoinHandle, time::Instant};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::client::IntoClientRequest,
    tungstenite::protocol::{Message as WebSocketMessage, WebSocketConfig},
    MaybeTlsStream, WebSocketStream,
};

use crate::compression::decompress_server_message;
use crate::db_connection::debug_log;
use crate::metrics::CLIENT_METRICS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum NegotiatedWsProtocol {
    #[default]
    V2,
    V3,
}

impl NegotiatedWsProtocol {
    /// Maps the negotiated websocket subprotocol string onto the transport
    /// framing rules understood by the native SDK.
    fn from_negotiated_protocol(protocol: &str) -> Self {
        match protocol {
            ws::v3::BIN_PROTOCOL => Self::V3,
            "" | ws::v2::BIN_PROTOCOL => Self::V2,
            unknown => {
                log::warn!(
                    "Unexpected websocket subprotocol \"{unknown}\", falling back to {}",
                    ws::v2::BIN_PROTOCOL
                );
                Self::V2
            }
        }
    }
}

const V3_PREFERRED_PROTOCOLS: [&str; 2] = [ws::v3::BIN_PROTOCOL, ws::v2::BIN_PROTOCOL];
/// Maximum encoded size for one native outbound v3 transport frame.
const MAX_V3_OUTBOUND_FRAME_BYTES: usize = 256 * 1024;
const BSATN_SUM_TAG_BYTES: usize = 1;
const BSATN_LENGTH_PREFIX_BYTES: usize = 4;

fn make_uri(host: Uri, db_name: &str, connection_id: Option<ConnectionId>, params: WsParams) -> Result<Uri, UriError> {
    make_uri_impl(host, db_name, connection_id, params, None)
}

// Tungstenite doesn't offer an interface to specify a WebSocket protocol, which frankly
// seems like a pretty glaring omission in its API. In order to insert our own protocol
// header, we manually the `Request` constructed by
// `tungstenite::IntoClientRequest::into_client_request`.

// TODO: `core` uses [Hyper](https://docs.rs/hyper/latest/hyper/) as its HTTP library
//       rather than having Tungstenite manage its own connections. Should this library do
//       the same?

fn make_request(
    host: Uri,
    db_name: &str,
    token: Option<&str>,
    connection_id: Option<ConnectionId>,
    params: WsParams,
) -> Result<http::Request<()>, WsError> {
    let uri = make_uri(host, db_name, connection_id, params)?;
    let mut req = IntoClientRequest::into_client_request(uri.clone()).map_err(|source| WsError::Tungstenite {
        uri,
        source: Arc::new(source),
    })?;
    request_insert_protocol_header(&mut req);
    request_insert_auth_header(&mut req, token);
    Ok(req)
}

fn request_insert_protocol_header(req: &mut http::Request<()>) {
    // Prefer v3 for transport batching, but continue advertising v2 so older
    // servers can negotiate the legacy wire format unchanged.
    req.headers_mut().insert(
        http::header::SEC_WEBSOCKET_PROTOCOL,
        http::HeaderValue::from_str(&V3_PREFERRED_PROTOCOLS.join(", ")).unwrap(),
    );
}

fn request_insert_auth_header(req: &mut http::Request<()>, token: Option<&str>) {
    if let Some(token) = token {
        let auth = ["Bearer ", token].concat().try_into().unwrap();
        req.headers_mut().insert(http::header::AUTHORIZATION, auth);
    }
}

/// Expands a v3 server frame into the ordered sequence of encoded inner v2
/// server messages it carries.
fn flatten_server_frame(frame: ws::v3::ServerFrame) -> Box<[Bytes]> {
    match frame {
        ws::v3::ServerFrame::Single(message) => Box::new([message]),
        ws::v3::ServerFrame::Batch(messages) => messages,
    }
}

/// Wraps one or more encoded v2 client messages in a v3 transport frame.
fn encode_v3_client_frame(messages: Vec<Bytes>) -> Bytes {
    let frame = if messages.len() == 1 {
        ws::v3::ClientFrame::Single(messages.into_iter().next().unwrap())
    } else {
        ws::v3::ClientFrame::Batch(messages.into_boxed_slice())
    };
    Bytes::from(bsatn::to_vec(&frame).expect("should be able to bsatn encode v3 client frame"))
}

/// Returns the encoded size of a v3 `Single` frame carrying `message`.
fn encoded_v3_single_frame_size(message: &Bytes) -> usize {
    BSATN_SUM_TAG_BYTES + BSATN_LENGTH_PREFIX_BYTES + message.len()
}

/// Returns the encoded size of a v3 `Batch` frame containing only its first
/// logical message.
fn encoded_v3_batch_frame_size_for_first_message(message: &Bytes) -> usize {
    BSATN_SUM_TAG_BYTES + BSATN_LENGTH_PREFIX_BYTES + BSATN_LENGTH_PREFIX_BYTES + message.len()
}

/// Returns the encoded contribution of one additional logical message inside a
/// v3 `Batch` frame.
fn encoded_v3_batch_element_size(message: &Bytes) -> usize {
    BSATN_LENGTH_PREFIX_BYTES + message.len()
}

/// Builds one bounded v3 transport frame from `first_message` and as many
/// queued logical messages as fit under the configured frame-size cap.
fn encode_v3_outbound_frame<F>(
    first_message: ws::v2::ClientMessage,
    pending_outgoing: &mut VecDeque<ws::v2::ClientMessage>,
    mut try_next_outgoing_now: F,
) -> Bytes
where
    F: FnMut() -> Option<ws::v2::ClientMessage>,
{
    let first_message = encode_v2_client_message_bytes(&first_message);
    // Oversized logical messages are still sent alone so they cannot block the
    // queue forever behind the frame-size limit.
    if encoded_v3_single_frame_size(&first_message) > MAX_V3_OUTBOUND_FRAME_BYTES {
        if pending_outgoing.is_empty()
            && let Some(next_message) = try_next_outgoing_now()
        {
            pending_outgoing.push_front(next_message);
        }

        return encode_v3_client_frame(vec![first_message]);
    }

    let mut messages = vec![first_message];
    let mut batch_size = encoded_v3_batch_frame_size_for_first_message(messages.first().unwrap());

    loop {
        let Some(next_message) = pending_outgoing.pop_front().or_else(&mut try_next_outgoing_now) else {
            break;
        };
        let next_message_bytes = encode_v2_client_message_bytes(&next_message);
        let next_batch_size = batch_size + encoded_v3_batch_element_size(&next_message_bytes);
        if next_batch_size > MAX_V3_OUTBOUND_FRAME_BYTES {
            pending_outgoing.push_front(next_message);
            break;
        }
        batch_size = next_batch_size;
        messages.push(next_message_bytes);
    }

    encode_v3_client_frame(messages)
}

/// Encodes the next outbound logical message according to the negotiated
/// transport and reports whether a capped v3 flush left queued work behind.
fn encode_outgoing_message<F>(
    protocol: NegotiatedWsProtocol,
    first_message: ws::v2::ClientMessage,
    pending_outgoing: &mut VecDeque<ws::v2::ClientMessage>,
    try_next_outgoing_now: F,
) -> (Bytes, bool)
where
    F: FnMut() -> Option<ws::v2::ClientMessage>,
{
    match protocol {
        NegotiatedWsProtocol::V2 => (encode_v2_client_message_bytes(&first_message), false),
        NegotiatedWsProtocol::V3 => {
            let frame = encode_v3_outbound_frame(first_message, pending_outgoing, try_next_outgoing_now);
            (frame, !pending_outgoing.is_empty())
        }
    }
}

/// Parses one native websocket payload and forwards each decoded logical v2
/// server message to the SDK's inbound queue, logging decode or enqueue
/// failures locally.
fn forward_parsed_responses_native(
    protocol: NegotiatedWsProtocol,
    incoming_messages: &mpsc::UnboundedSender<ws::v2::ServerMessage>,
    extra_logging: &Option<Arc<Mutex<File>>>,
    bytes: &[u8],
) {
    match WsConnection::parse_responses(protocol, bytes) {
        Err(e) => {
            debug_log(extra_logging, |file| {
                writeln!(file, "Error decoding WebSocketMessage::Binary payload: {e:?}")
            });
            log::warn!("Error decoding WebSocketMessage::Binary payload: {e:?}");
        }
        Ok(messages) => {
            for msg in messages {
                if let Err(e) = incoming_messages.unbounded_send(msg) {
                    debug_log(extra_logging, |file| {
                        writeln!(file, "Error sending decoded message to incoming_messages queue: {e:?}")
                    });
                    log::warn!("Error sending decoded message to incoming_messages queue: {e:?}");
                }
            }
        }
    }
}

/// If `res` evaluates to `Err(e)`, log a warning in the form `"{}: {:?}", $cause, e`.
///
/// Could be trivially written as a function, but macro-ifying it preserves the source location of the log.
macro_rules! maybe_log_error {
    ($extra_logging:expr, $cause:expr, $res:expr) => {
        if let Err(e) = $res {
            let cause = $cause;
            debug_log($extra_logging, |file| writeln!(file, "{}: {:?}", cause, e));
            log::warn!("{}: {:?}", cause, e);
        }
    };
}

impl WsConnection {
    pub(crate) async fn connect(
        host: Uri,
        db_name: &str,
        token: Option<&str>,
        connection_id: Option<ConnectionId>,
        params: WsParams,
    ) -> Result<Self, WsError> {
        let req = make_request(host, db_name, token, connection_id, params)?;

        // Grab the URI for error-reporting.
        let uri = req.uri().clone();

        let (sock, response): (WebSocketStream<MaybeTlsStream<TcpStream>>, _) = connect_async_with_config(
            req,
            // TODO(kim): In order to be able to replicate module WASM blobs,
            // `cloud-next` cannot have message / frame size limits. That's
            // obviously a bad default for all other clients, though.
            Some(WebSocketConfig::default().max_frame_size(None).max_message_size(None)),
            false,
        )
        .await
        .map_err(|source| WsError::Tungstenite {
            uri,
            source: Arc::new(source),
        })?;
        let negotiated_protocol = response
            .headers()
            .get(http::header::SEC_WEBSOCKET_PROTOCOL)
            .and_then(|protocol| protocol.to_str().ok())
            .map(NegotiatedWsProtocol::from_negotiated_protocol)
            .unwrap_or_default();
        Ok(WsConnection {
            db_name: db_name.into(),
            protocol: negotiated_protocol,
            sock,
        })
    }

    /// Parses one native websocket payload into the ordered logical v2 server
    /// messages carried by the negotiated transport.
    fn parse_responses(protocol: NegotiatedWsProtocol, bytes: &[u8]) -> Result<Vec<ws::v2::ServerMessage>, WsError> {
        let bytes = &*decompress_server_message(bytes)?;
        match protocol {
            NegotiatedWsProtocol::V2 => Ok(vec![decode_v2_server_message(bytes)?]),
            NegotiatedWsProtocol::V3 => {
                let frame: ws::v3::ServerFrame =
                    bsatn::from_slice(bytes).map_err(|source| WsError::DeserializeMessage { source })?;
                flatten_server_frame(frame)
                    .into_vec()
                    .into_iter()
                    .map(|message| decode_v2_server_message(&message))
                    .collect()
            }
        }
    }

    async fn message_loop(
        mut self,
        incoming_messages: mpsc::UnboundedSender<ws::v2::ServerMessage>,
        outgoing_messages: mpsc::UnboundedReceiver<ws::v2::ClientMessage>,
        extra_logging: Option<Arc<Mutex<File>>>,
    ) {
        let websocket_received = CLIENT_METRICS.websocket_received.with_label_values(&self.db_name);
        let websocket_received_msg_size = CLIENT_METRICS
            .websocket_received_msg_size
            .with_label_values(&self.db_name);
        let record_metrics = |msg_size: usize| {
            websocket_received.inc();
            websocket_received_msg_size.observe(msg_size as f64);
        };

        // There is a small but plausible chance that a client's socket will not
        // be notified that the remote end has closed the connection, e.g.
        // because of the remote machine being power cycled, or middleboxes
        // misbehaving.
        //
        // Unless the client uses dynamic subscriptions, it will only ever try
        // to read from the socket, and thus not notice the connection closure.
        //
        // For certain types of clients it is crucial to eventually time out
        // such connections, and attempt to reconnect. We don't, however, want
        // to flood the server with `Ping` frames unnecessarily.
        //
        // Instead, we:
        //
        // * Check every `IDLE_TIMEOUT` whether some data has arrived.
        //
        //   - If not, send a `Ping` frame.
        //
        // * Check after another `IDLE_TIMEOUT` whether data has arrived.
        //
        //   - If not, and we were expecting a `Pong` response, consider the
        //     connection bad and exit the loop, thereby closing the socket.
        //
        // Note that the server also initiates `Ping`s, currently at `2 * IDLE_TIMEOUT`.
        // If both ends cannot communicate, we assume the server has already
        // timed out the client, and so don't bother sending a `Close` frame.
        const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
        let mut idle_timeout_interval = tokio::time::interval_at(Instant::now() + IDLE_TIMEOUT, IDLE_TIMEOUT);

        let mut idle = true;
        let mut want_pong = false;

        let mut outgoing_messages = Some(outgoing_messages);
        let mut pending_outgoing = VecDeque::new();
        let mut yield_after_capped_flush = false;
        loop {
            if yield_after_capped_flush {
                // Under v3 we emit at most one bounded frame per flush. If there
                // are still queued messages after hitting the cap, yield before
                // sending the next frame so inbound socket work is not starved by
                // a tight outbound-only drain loop.
                yield_after_capped_flush = false;
                tokio::task::yield_now().await;
            }
            tokio::select! {
                incoming = self.sock.try_next() => match incoming {
                    Err(tokio_tungstenite::tungstenite::error::Error::ConnectionClosed) | Ok(None) => {
                        log::info!("Connection closed");
                        break;
                    },

                    Err(e) => {
                        maybe_log_error!(
                            &extra_logging,
                            "Error reading message from read WebSocket stream",
                            Result::<(), _>::Err(e)
                        );
                        break;
                    },

                    Ok(Some(WebSocketMessage::Binary(bytes))) => {
                        idle = false;
                        record_metrics(bytes.len());
                        forward_parsed_responses_native(self.protocol, &incoming_messages, &extra_logging, &bytes);
                    }

                    Ok(Some(WebSocketMessage::Ping(payload))) => {
                        log::trace!("received ping");
                        idle = false;
                        record_metrics(payload.len());
                        // No need to explicitly respond with a `Pong`,
                        // as tungstenite handles this automatically.
                        // See [https://github.com/snapview/tokio-tungstenite/issues/88].
                    },

                    Ok(Some(WebSocketMessage::Pong(payload))) => {
                        log::trace!("received pong");
                        idle = false;
                        want_pong = false;
                        record_metrics(payload.len());
                    },

                    Ok(Some(other)) => {
                        debug_log(&extra_logging, |file| writeln!(file, "Unexpeccted WebSocket message {other:?}"));
                        log::warn!("Unexpected WebSocket message {other:?}");
                        idle = false;
                        record_metrics(other.len());
                    },
                },

                _ = idle_timeout_interval.tick() => {
                    if mem::replace(&mut idle, true) {
                        if want_pong {
                            // Nothing received while we were waiting for a pong.
                            debug_log(&extra_logging, |file| writeln!(file, "Connection timed out"));
                            log::warn!("Connection timed out");
                            break;
                        }

                        log::trace!("sending client ping");
                        let ping = WebSocketMessage::Ping(Bytes::new());
                        if let Err(e) = self.sock.send(ping).await {
                            debug_log(&extra_logging, |file| writeln!(file, "Error sending ping: {e:?}"));
                            log::warn!("Error sending ping: {e:?}");
                            break;
                        }
                        want_pong = true;
                    }
                },

                // this is stupid. we want to handle the channel close *once*, and then disable this branch
                Some(outgoing) = async {
                    Some(if let Some(outgoing) = pending_outgoing.pop_front() {
                        Some(outgoing)
                    } else {
                        outgoing_messages.as_mut()?.next().await
                    })
                } => match outgoing {
                    Some(outgoing) => {
                        let (msg, has_leftover_pending_outgoing) = encode_outgoing_message(
                            self.protocol,
                            outgoing,
                            &mut pending_outgoing,
                            || outgoing_messages.as_mut().and_then(|outgoing| outgoing.try_next().ok().flatten()),
                        );
                        if let Err(e) = self.sock.send(WebSocketMessage::Binary(msg)).await {
                            debug_log(&extra_logging, |file| writeln!(file, "Error sending outgoing message: {e:?}"));
                            log::warn!("Error sending outgoing message: {e:?}");
                            break;
                        }
                        yield_after_capped_flush = has_leftover_pending_outgoing;
                    }
                    None => {
                        maybe_log_error!(&extra_logging, "Error sending close frame", SinkExt::close(&mut self.sock).await);
                        outgoing_messages = None;
                    }
                },
            }
        }
    }

    pub(crate) fn spawn_message_loop(
        self,
        runtime: &runtime::Handle,
        extra_logging: Option<Arc<Mutex<File>>>,
    ) -> (
        JoinHandle<()>,
        mpsc::UnboundedReceiver<ws::v2::ServerMessage>,
        mpsc::UnboundedSender<ws::v2::ClientMessage>,
    ) {
        let (outgoing_send, outgoing_recv) = mpsc::unbounded();
        let (incoming_send, incoming_recv) = mpsc::unbounded();
        let handle = runtime.spawn(self.message_loop(incoming_send, outgoing_recv, extra_logging));
        (handle, incoming_recv, outgoing_send)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_lib::Identity;
    use spacetimedb_lib::{TimeDuration, Timestamp};

    fn reducer_call(request_id: u32, arg_len: usize) -> ws::v2::ClientMessage {
        ws::v2::ClientMessage::CallReducer(ws::v2::CallReducer {
            request_id,
            flags: ws::v2::CallReducerFlags::Default,
            reducer: "reducer".into(),
            args: Bytes::from(vec![0; arg_len]),
        })
    }

    fn procedure_result(request_id: u32) -> ws::v2::ServerMessage {
        ws::v2::ServerMessage::ProcedureResult(ws::v2::ProcedureResult {
            status: ws::v2::ProcedureStatus::Returned(Bytes::new()),
            timestamp: Timestamp::UNIX_EPOCH,
            total_host_execution_duration: TimeDuration::ZERO,
            request_id,
        })
    }

    fn encode_server_message(message: &ws::v2::ServerMessage) -> Vec<u8> {
        let mut encoded = vec![ws::common::SERVER_MSG_COMPRESSION_TAG_NONE];
        encoded.extend(bsatn::to_vec(message).unwrap());
        encoded
    }

    fn encode_server_frame(frame: &ws::v3::ServerFrame) -> Vec<u8> {
        let mut encoded = vec![ws::common::SERVER_MSG_COMPRESSION_TAG_NONE];
        encoded.extend(bsatn::to_vec(frame).unwrap());
        encoded
    }

    #[test]
    fn negotiated_protocol_defaults_to_v2() {
        assert_eq!(
            NegotiatedWsProtocol::from_negotiated_protocol(""),
            NegotiatedWsProtocol::V2
        );
        assert_eq!(
            NegotiatedWsProtocol::from_negotiated_protocol(ws::v2::BIN_PROTOCOL),
            NegotiatedWsProtocol::V2
        );
        assert_eq!(
            NegotiatedWsProtocol::from_negotiated_protocol("unexpected-protocol"),
            NegotiatedWsProtocol::V2
        );
    }

    #[test]
    fn negotiated_protocol_recognizes_v3() {
        assert_eq!(
            NegotiatedWsProtocol::from_negotiated_protocol(ws::v3::BIN_PROTOCOL),
            NegotiatedWsProtocol::V3
        );
    }

    #[test]
    fn encode_outgoing_message_batches_small_v3_messages() {
        let mut pending = VecDeque::new();
        let (raw, has_leftover_pending_outgoing) =
            encode_outgoing_message(NegotiatedWsProtocol::V3, reducer_call(1, 8), &mut pending, {
                let mut extra = VecDeque::from([reducer_call(2, 8)]);
                move || extra.pop_front()
            });

        assert!(!has_leftover_pending_outgoing);
        assert!(pending.is_empty());

        let frame: ws::v3::ClientFrame = bsatn::from_slice(&raw).unwrap();
        let ws::v3::ClientFrame::Batch(messages) = frame else {
            panic!("expected batched v3 client frame");
        };
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn encode_outgoing_message_caps_v3_frames_at_256_kib() {
        let mut pending = VecDeque::new();
        let oversized = 200 * 1024;
        let (raw, has_leftover_pending_outgoing) =
            encode_outgoing_message(NegotiatedWsProtocol::V3, reducer_call(1, oversized), &mut pending, {
                let mut extra = VecDeque::from([reducer_call(2, oversized)]);
                move || extra.pop_front()
            });

        assert!(has_leftover_pending_outgoing);
        assert_eq!(pending.len(), 1);

        let frame: ws::v3::ClientFrame = bsatn::from_slice(&raw).unwrap();
        let ws::v3::ClientFrame::Single(message) = frame else {
            panic!("expected single v3 client frame");
        };
        let inner: ws::v2::ClientMessage = bsatn::from_slice(&message).unwrap();
        match inner {
            ws::v2::ClientMessage::CallReducer(call) => assert_eq!(call.request_id, 1),
            _ => panic!("expected CallReducer inner message"),
        }
    }

    #[test]
    fn parse_response_supports_v2_messages() {
        let encoded = encode_server_message(&ws::v2::ServerMessage::InitialConnection(ws::v2::InitialConnection {
            identity: Identity::ZERO,
            connection_id: ConnectionId::ZERO,
            token: "token".into(),
        }));

        let messages = WsConnection::parse_responses(NegotiatedWsProtocol::V2, &encoded).unwrap();
        assert_eq!(messages.len(), 1);
        match &messages[0] {
            ws::v2::ServerMessage::InitialConnection(message) => {
                assert_eq!(message.identity, Identity::ZERO);
                assert_eq!(message.connection_id, ConnectionId::ZERO);
            }
            other => panic!("unexpected v2 message: {other:?}"),
        }
    }

    #[test]
    fn parse_response_unwraps_v3_batches() {
        let first = procedure_result(1);
        let second = procedure_result(2);
        let frame = ws::v3::ServerFrame::Batch(
            vec![
                Bytes::from(bsatn::to_vec(&first).unwrap()),
                Bytes::from(bsatn::to_vec(&second).unwrap()),
            ]
            .into_boxed_slice(),
        );
        let encoded = encode_server_frame(&frame);

        let messages = WsConnection::parse_responses(NegotiatedWsProtocol::V3, &encoded).unwrap();
        assert_eq!(messages.len(), 2);
        for (expected_request_id, message) in [1, 2].into_iter().zip(messages) {
            match message {
                ws::v2::ServerMessage::ProcedureResult(result) => {
                    assert_eq!(result.request_id, expected_request_id);
                }
                other => panic!("unexpected v3 inner message: {other:?}"),
            }
        }
    }
}
