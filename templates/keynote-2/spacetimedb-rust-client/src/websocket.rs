//! Low-level WebSocket plumbing.
//!
//! This module is internal, and may incompatibly change without warning.

use bytes::Bytes;
use futures::{SinkExt, TryStreamExt};
use http::uri::{InvalidUri, Scheme, Uri};
use spacetimedb_client_api_messages::websocket::common::SERVER_MSG_COMPRESSION_TAG_NONE;
use spacetimedb_client_api_messages::websocket::v1::{BIN_PROTOCOL, ClientMessage, Compression};
use spacetimedb_lib::de::Deserialize;
use spacetimedb_lib::{ConnectionId, bsatn};
use std::mem;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio::{net::TcpStream, runtime};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::client::IntoClientRequest,
    tungstenite::protocol::{Message as WebSocketMessage, WebSocketConfig},
};

fn decompress_server_message(raw: &[u8]) -> &[u8] {
    match raw {
        [SERVER_MSG_COMPRESSION_TAG_NONE, bytes @ ..] => bytes,
        _ => unimplemented!(),
    }
}

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

    #[error("Failed to deserialize WebSocket message: {source}")]
    DeserializeMessage {
        #[source]
        source: bsatn::DecodeError,
    },
}

pub struct WsConnection {
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
                });
            }
        },
        None => "ws".parse().unwrap(),
    })
}

#[derive(Clone, Copy, Default)]
pub struct WsParams {
    pub compression: Compression,
    pub light: bool,
    /// `Some(true)` to enable confirmed reads for the connection,
    /// `Some(false)` to disable them.
    /// `None` to not set the parameter and let the server choose.
    pub confirmed: Option<bool>,
}

pub fn make_uri(
    host: Uri,
    db_name: &str,
    connection_id: Option<ConnectionId>,
    params: WsParams,
) -> Result<Uri, UriError> {
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

    // Specify the desired compression for host->client replies.
    match params.compression {
        Compression::None => path.push_str("?compression=None"),
        Compression::Gzip => path.push_str("?compression=Gzip"),
        // The host uses the same default as the sdk,
        // but in case this changes, we prefer to be explicit now.
        Compression::Brotli => path.push_str("?compression=Brotli"),
    };

    // Provide the connection ID if the client provided one.
    if let Some(cid) = connection_id {
        // If a connection ID is provided, append it to the path.
        path.push_str("&connection_id=");
        path.push_str(&cid.to_hex());
    }

    // Specify the `light` mode if requested.
    if params.light {
        path.push_str("&light=true");
    }

    // Enable confirmed reads if requested.
    if let Some(confirmed) = params.confirmed {
        path.push_str("&confirmed=");
        path.push_str(if confirmed { "true" } else { "false" });
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

/// If `res` evaluates to `Err(e)`, log a warning in the form `"{}: {:?}", $cause, e`.
///
/// Could be trivially written as a function, but macro-ifying it preserves the source location of the log.
macro_rules! maybe_log_error {
    ($cause:expr, $res:expr) => {
        if let Err(e) = $res {
            log::warn!("{}: {:?}", $cause, e);
        }
    };
}

#[derive(Deserialize, PartialEq, Debug)]
pub enum ServerMessage {
    /// Informs of changes to subscribed rows.
    /// This will be removed when we switch to `SubscribeSingle`.
    InitialSubscription,
    /// Upon reducer run.
    TransactionUpdate,
    /// Upon reducer run, but limited to just the table updates.
    TransactionUpdateLight,
    /// After connecting, to inform client of its identity.
    IdentityToken,
    /// Return results to a one off SQL query.
    OneOffQueryResponse,
    /// Sent in response to a `SubscribeSingle` message. This contains the initial matching rows.
    SubscribeApplied,
    /// Sent in response to an `Unsubscribe` message. This contains the matching rows.
    UnsubscribeApplied,
    /// Communicate an error in the subscription lifecycle.
    SubscriptionError,
    /// Sent in response to a `SubscribeMulti` message. This contains the initial matching rows.
    SubscribeMultiApplied,
    /// Sent in response to an `UnsubscribeMulti` message. This contains the matching rows.
    UnsubscribeMultiApplied,
    /// Sent in response to a [`CallProcedure`] message. This contains the return value.
    ProcedureResult,
}

pub type Recv = mpsc::UnboundedReceiver<ServerMessage>;
pub type Send = mpsc::UnboundedSender<ClientMessage<Bytes>>;

impl WsConnection {
    pub async fn connect(
        host: Uri,
        db_name: &str,
        token: Option<&str>,
        connection_id: Option<ConnectionId>,
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
        Ok(WsConnection { sock })
    }

    fn parse_response(bytes: &[u8]) -> Result<ServerMessage, WsError> {
        let bytes = decompress_server_message(bytes);
        bsatn::from_slice(bytes).map_err(|source| WsError::DeserializeMessage { source })
    }

    fn encode_message(msg: ClientMessage<Bytes>) -> WebSocketMessage {
        WebSocketMessage::Binary(bsatn::to_vec(&msg).unwrap().into())
    }

    async fn message_loop(
        mut self,
        incoming_messages: mpsc::UnboundedSender<ServerMessage>,
        outgoing_messages: mpsc::UnboundedReceiver<ClientMessage<Bytes>>,
    ) {
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
                        maybe_log_error!(
                            "Error reading message from read WebSocket stream",
                            Result::<(), _>::Err(e)
                        );
                        break;
                    },

                    Ok(Some(WebSocketMessage::Binary(bytes))) => {
                        idle = false;
                        match Self::parse_response(&bytes) {
                            Err(e) => maybe_log_error!(
                                "Error decoding WebSocketMessage::Binary payload",
                                Result::<(), _>::Err(e)
                            ),
                            Ok(msg) => maybe_log_error!(
                                "Error sending decoded message to incoming_messages queue",
                                incoming_messages.send(msg)
                            ),
                        }
                    }

                    Ok(Some(WebSocketMessage::Ping(_))) => {
                        log::trace!("received ping");
                        idle = false;
                        // No need to explicitly respond with a `Pong`,
                        // as tungstenite handles this automatically.
                        // See [https://github.com/snapview/tokio-tungstenite/issues/88].
                    },

                    Ok(Some(WebSocketMessage::Pong(_))) => {
                        log::trace!("received pong");
                        idle = false;
                        want_pong = false;
                    },

                    Ok(Some(other)) => {
                        log::warn!("Unexpected WebSocket message {other:?}");
                        idle = false;
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
                Some(outgoing) = async { Some(outgoing_messages.as_mut()?.recv().await) } => match outgoing {
                    Some(outgoing) => {
                        let msg = Self::encode_message(outgoing);
                        if let Err(e) = self.sock.send(msg).await {
                            log::warn!("Error sending outgoing message: {e:?}");
                            break;
                        }
                    }
                    None => {
                        maybe_log_error!("Error sending close frame", SinkExt::close(&mut self.sock).await);
                        outgoing_messages = None;
                    }
                },
            }
        }
    }

    pub fn spawn_message_loop(
        self,
        runtime: &runtime::Handle,
    ) -> (
        JoinHandle<()>,
        mpsc::UnboundedReceiver<ServerMessage>,
        mpsc::UnboundedSender<ClientMessage<Bytes>>,
    ) {
        let (outgoing_send, outgoing_recv) = mpsc::unbounded_channel();
        let (incoming_send, incoming_recv) = mpsc::unbounded_channel();
        let handle = runtime.spawn(self.message_loop(incoming_send, outgoing_recv));
        (handle, incoming_recv, outgoing_send)
    }
}
