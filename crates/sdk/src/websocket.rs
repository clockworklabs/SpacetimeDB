//! Low-level WebSocket plumbing.
//!
//! This module is internal, and may incompatibly change without warning.

use std::mem;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures::{SinkExt, StreamExt as _, TryStreamExt};
use futures_channel::mpsc;
use http::uri::{InvalidUri, Scheme, Uri};
use spacetimedb_client_api_messages::websocket::{
    brotli_decompress, gzip_decompress, BsatnFormat, Compression, BIN_PROTOCOL, SERVER_MSG_COMPRESSION_TAG_BROTLI,
    SERVER_MSG_COMPRESSION_TAG_GZIP, SERVER_MSG_COMPRESSION_TAG_NONE,
};
use spacetimedb_client_api_messages::websocket::{ClientMessage, ServerMessage};
use spacetimedb_lib::{bsatn, ConnectionId};
use thiserror::Error;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio::{net::TcpStream, runtime};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::client::IntoClientRequest,
    tungstenite::protocol::{Message as WebSocketMessage, WebSocketConfig},
    MaybeTlsStream, WebSocketStream,
};

use crate::metrics::CLIENT_METRICS;

#[derive(Error, Debug, Clone)]
pub enum UriError {
    #[error("Unknown URI scheme {scheme}, expected http, https, ws or wss")]
    UnknownUriScheme { scheme: String },

    #[error("Expected a URI without a query part, but found {query}")]
    UnexpectedQuery { query: String },

    #[error(transparent)]
    InvalidUri {
        // `Arc` is required for `Self: Clone`, as `http::uri::InvalidUri: !Clone`.
        source: Arc<http::uri::InvalidUri>,
    },

    #[error(transparent)]
    InvalidUriParts {
        // `Arc` is required for `Self: Clone`, as `http::uri::InvalidUriParts: !Clone`.
        source: Arc<http::uri::InvalidUriParts>,
    },
}

#[derive(Error, Debug, Clone)]
pub enum WsError {
    #[error(transparent)]
    UriError(#[from] UriError),

    #[error("Error in WebSocket connection with {uri}: {source}")]
    Tungstenite {
        uri: Uri,
        #[source]
        // `Arc` is required for `Self: Clone`, as `tungstenite::Error: !Clone`.
        source: Arc<tokio_tungstenite::tungstenite::Error>,
    },

    #[error("Received empty raw message, but valid messages always start with a one-byte compression flag")]
    EmptyMessage,

    #[error("Failed to deserialize WebSocket message: {source}")]
    DeserializeMessage {
        #[source]
        source: bsatn::DecodeError,
    },

    #[error("Failed to decompress WebSocket message with {scheme}: {source}")]
    Decompress {
        scheme: &'static str,
        #[source]
        // `Arc` is required for `Self: Clone`, as `std::io::Error: !Clone`.
        source: Arc<std::io::Error>,
    },

    #[error("Unrecognized compression scheme: {scheme:#x}")]
    UnknownCompressionScheme { scheme: u8 },
}

pub(crate) struct WsConnection {
    db_name: Box<str>,
    connection_id: ConnectionId,
    sock: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

fn parse_scheme(scheme: Option<Scheme>) -> Result<Scheme, UriError> {
    Ok(match scheme {
        Some(s) => match s.as_str() {
            "ws" | "wss" => s,
            "http" => "ws".parse().unwrap(),
            "https" => "wss".parse().unwrap(),
            unknown_scheme => {
                return Err(UriError::UnknownUriScheme {
                    scheme: unknown_scheme.into(),
                })
            }
        },
        None => "ws".parse().unwrap(),
    })
}

#[derive(Clone, Copy, Default)]
pub(crate) struct WsParams {
    pub compression: Compression,
    pub light: bool,
}

fn make_uri(host: Uri, db_name: &str, connection_id: ConnectionId, params: WsParams) -> Result<Uri, UriError> {
    let mut parts = host.into_parts();
    let scheme = parse_scheme(parts.scheme.take())?;
    parts.scheme = Some(scheme);
    let mut path = if let Some(path_and_query) = parts.path_and_query {
        if let Some(query) = path_and_query.query() {
            return Err(UriError::UnexpectedQuery { query: query.into() });
        }
        path_and_query.path().to_string()
    } else {
        "/".to_string()
    };

    // Normalize the path, ensuring it ends with `/`.
    if !path.ends_with('/') {
        path.push('/');
    }

    path.push_str("v1/database/");
    path.push_str(db_name);
    path.push_str("/subscribe");

    // Provide the connection ID.
    path.push_str("?connection_id=");
    path.push_str(&connection_id.to_hex());

    // Specify the desired compression for host->client replies.
    match params.compression {
        Compression::None => path.push_str("&compression=None"),
        Compression::Gzip => path.push_str("&compression=Gzip"),
        // The host uses the same default as the sdk,
        // but in case this changes, we prefer to be explicit now.
        Compression::Brotli => path.push_str("&compression=Brotli"),
    };

    // Specify the `light` mode if requested.
    if params.light {
        path.push_str("&light=true");
    }

    parts.path_and_query = Some(path.parse().map_err(|source: InvalidUri| UriError::InvalidUri {
        source: Arc::new(source),
    })?);
    Uri::from_parts(parts).map_err(|source| UriError::InvalidUriParts {
        source: Arc::new(source),
    })
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
    connection_id: ConnectionId,
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
    req.headers_mut().insert(
        http::header::SEC_WEBSOCKET_PROTOCOL,
        const { http::HeaderValue::from_static(BIN_PROTOCOL) },
    );
}

fn request_insert_auth_header(req: &mut http::Request<()>, token: Option<&str>) {
    if let Some(token) = token {
        let auth = ["Bearer ", token].concat().try_into().unwrap();
        req.headers_mut().insert(http::header::AUTHORIZATION, auth);
    }
}

impl WsConnection {
    pub(crate) async fn connect(
        host: Uri,
        db_name: &str,
        token: Option<&str>,
        connection_id: ConnectionId,
        params: WsParams,
    ) -> Result<Self, WsError> {
        let req = make_request(host, db_name, token, connection_id, params)?;

        // Grab the URI for error-reporting.
        let uri = req.uri().clone();

        let (sock, _): (WebSocketStream<MaybeTlsStream<TcpStream>>, _) = connect_async_with_config(
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
        Ok(WsConnection {
            db_name: db_name.into(),
            connection_id,
            sock,
        })
    }

    pub(crate) fn parse_response(bytes: &[u8]) -> Result<ServerMessage<BsatnFormat>, WsError> {
        let (compression, bytes) = bytes.split_first().ok_or(WsError::EmptyMessage)?;

        Ok(match *compression {
            SERVER_MSG_COMPRESSION_TAG_NONE => {
                bsatn::from_slice(bytes).map_err(|source| WsError::DeserializeMessage { source })?
            }
            SERVER_MSG_COMPRESSION_TAG_BROTLI => {
                bsatn::from_slice(&brotli_decompress(bytes).map_err(|source| WsError::Decompress {
                    scheme: "brotli",
                    source: Arc::new(source),
                })?)
                .map_err(|source| WsError::DeserializeMessage { source })?
            }
            SERVER_MSG_COMPRESSION_TAG_GZIP => {
                bsatn::from_slice(&gzip_decompress(bytes).map_err(|source| WsError::Decompress {
                    scheme: "gzip",
                    source: Arc::new(source),
                })?)
                .map_err(|source| WsError::DeserializeMessage { source })?
            }
            c => {
                return Err(WsError::UnknownCompressionScheme { scheme: c });
            }
        })
    }

    pub(crate) fn encode_message(msg: ClientMessage<Bytes>) -> WebSocketMessage {
        WebSocketMessage::Binary(bsatn::to_vec(&msg).unwrap().into())
    }

    fn maybe_log_error<T, U: std::fmt::Debug>(cause: &str, res: std::result::Result<T, U>) {
        if let Err(e) = res {
            log::warn!("{}: {:?}", cause, e);
        }
    }

    async fn message_loop(
        mut self,
        incoming_messages: mpsc::UnboundedSender<ServerMessage<BsatnFormat>>,
        outgoing_messages: mpsc::UnboundedReceiver<ClientMessage<Bytes>>,
    ) {
        let websocket_received = CLIENT_METRICS
            .websocket_received
            .with_label_values(&self.db_name, &self.connection_id);
        let websocket_received_msg_size = CLIENT_METRICS
            .websocket_received_msg_size
            .with_label_values(&self.db_name, &self.connection_id);
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
        loop {
            tokio::select! {
                incoming = self.sock.try_next() => match incoming {
                    Err(tokio_tungstenite::tungstenite::error::Error::ConnectionClosed) | Ok(None) => {
                        log::info!("Connection closed");
                        break;
                    },

                    Err(e) => {
                        Self::maybe_log_error::<(), _>(
                            "Error reading message from read WebSocket stream",
                            Err(e),
                        );
                        break;
                    },

                    Ok(Some(WebSocketMessage::Binary(bytes))) => {
                        idle = false;
                        record_metrics(bytes.len());
                        match Self::parse_response(&bytes) {
                            Err(e) => Self::maybe_log_error::<(), _>(
                                "Error decoding WebSocketMessage::Binary payload",
                                Err(e),
                            ),
                            Ok(msg) => Self::maybe_log_error(
                                "Error sending decoded message to incoming_messages queue",
                                incoming_messages.unbounded_send(msg),
                            ),
                        }
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
                        log::warn!("Unexpected WebSocket message {:?}", other);
                        idle = false;
                        record_metrics(other.len());
                    },
                },

                _ = idle_timeout_interval.tick() => {
                    if mem::replace(&mut idle, true) {
                        if want_pong {
                            // Nothing received while we were waiting for a pong.
                            log::warn!("Connection timed out");
                            break;
                        }

                        log::trace!("sending client ping");
                        let ping = WebSocketMessage::Ping(Bytes::new());
                        if let Err(e) = self.sock.send(ping).await {
                            log::warn!("Error sending ping: {e:?}");
                            break;
                        }
                        want_pong = true;
                    }
                },

                // this is stupid. we want to handle the channel close *once*, and then disable this branch
                Some(outgoing) = async { Some(outgoing_messages.as_mut()?.next().await) } => match outgoing {
                    Some(outgoing) => {
                        let msg = Self::encode_message(outgoing);
                        if let Err(e) = self.sock.send(msg).await {
                            log::warn!("Error sending outgoing message: {e:?}");
                            break;
                        }
                    }
                    None => {
                        Self::maybe_log_error("Error sending close frame", SinkExt::close(&mut self.sock).await);
                        outgoing_messages = None;
                    }
                },
            }
        }
    }

    pub(crate) fn spawn_message_loop(
        self,
        runtime: &runtime::Handle,
    ) -> (
        JoinHandle<()>,
        mpsc::UnboundedReceiver<ServerMessage<BsatnFormat>>,
        mpsc::UnboundedSender<ClientMessage<Bytes>>,
    ) {
        let (outgoing_send, outgoing_recv) = mpsc::unbounded();
        let (incoming_send, incoming_recv) = mpsc::unbounded();
        let handle = runtime.spawn(self.message_loop(incoming_send, outgoing_recv));
        (handle, incoming_recv, outgoing_send)
    }
}
