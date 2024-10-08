//! Low-level WebSocket plumbing.
//!
//! This module is internal, and may incompatibly change without warning.

use crate::ws_messages::{ClientMessage, ServerMessage};
use anyhow::{anyhow, bail, Context, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt as _, TryStreamExt};
use futures_channel::mpsc;
use http::uri::{Scheme, Uri};
use spacetimedb_client_api_messages::websocket::{
    brotli_decompress, gzip_decompress, BsatnFormat, Compression, SERVER_MSG_COMPRESSION_TAG_BROTLI,
    SERVER_MSG_COMPRESSION_TAG_GZIP, SERVER_MSG_COMPRESSION_TAG_NONE,
};
use spacetimedb_lib::{bsatn, Address, Identity};
use tokio::task::JoinHandle;
use tokio::{net::TcpStream, runtime};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::client::IntoClientRequest,
    tungstenite::protocol::{Message as WebSocketMessage, WebSocketConfig},
    MaybeTlsStream, WebSocketStream,
};

pub(crate) struct WsConnection {
    sock: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

fn parse_scheme(scheme: Option<Scheme>) -> Result<Scheme> {
    Ok(match scheme {
        Some(s) => match s.as_str() {
            "ws" | "wss" => s,
            "http" => "ws".parse()?,
            "https" => "wss".parse()?,
            unknown_scheme => bail!("Unknown URI scheme {}", unknown_scheme),
        },
        None => "ws".parse()?,
    })
}

#[derive(Clone, Copy, Default)]
pub(crate) struct WsParams {
    pub compression: Compression,
    pub light: bool,
}

fn make_uri<Host>(host: Host, db_name: &str, client_address: Address, params: WsParams) -> Result<Uri>
where
    Host: TryInto<Uri>,
    <Host as TryInto<Uri>>::Error: std::error::Error + Send + Sync + 'static,
{
    let host: Uri = host.try_into()?;
    let mut parts = host.into_parts();
    let scheme = parse_scheme(parts.scheme.take())?;
    parts.scheme = Some(scheme);
    let mut path = if let Some(path_and_query) = parts.path_and_query {
        if let Some(query) = path_and_query.query() {
            bail!("Unexpected query {}", query);
        }
        path_and_query.path().to_string()
    } else {
        "/".to_string()
    };

    // Normalize the path, ensuring it ends with `/`.
    if !path.ends_with('/') {
        path.push('/');
    }

    path.push_str("database/subscribe/");
    path.push_str(db_name);

    // Provide the client address.
    path.push_str("?client_address=");
    path.push_str(&client_address.to_hex());

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

    parts.path_and_query = Some(path.parse()?);
    Ok(Uri::from_parts(parts)?)
}

// Tungstenite doesn't offer an interface to specify a WebSocket protocol, which frankly
// seems like a pretty glaring omission in its API. In order to insert our own protocol
// header, we manually the `Request` constructed by
// `tungstenite::IntoClientRequest::into_client_request`.

// TODO: `core` uses [Hyper](https://docs.rs/hyper/latest/hyper/) as its HTTP library
//       rather than having Tungstenite manage its own connections. Should this library do
//       the same?

fn make_request<Host>(
    host: Host,
    db_name: &str,
    credentials: Option<&(Identity, String)>,
    client_address: Address,
    params: WsParams,
) -> Result<http::Request<()>>
where
    Host: TryInto<Uri>,
    <Host as TryInto<Uri>>::Error: std::error::Error + Send + Sync + 'static,
{
    let uri = make_uri(host, db_name, client_address, params)?;
    let mut req = IntoClientRequest::into_client_request(uri)?;
    request_insert_protocol_header(&mut req);
    request_insert_auth_header(&mut req, credentials);
    Ok(req)
}

fn request_add_header(req: &mut http::Request<()>, key: &'static str, val: http::header::HeaderValue) {
    let _prev = req.headers_mut().insert(key, val);
    debug_assert!(_prev.is_none(), "HttpRequest already had {:?} header {:?}", key, _prev,);
}

const PROTOCOL_HEADER_KEY: &str = "Sec-WebSocket-Protocol";
const PROTOCOL_HEADER_VALUE: &str = "v1.bsatn.spacetimedb";

fn request_insert_protocol_header(req: &mut http::Request<()>) {
    request_add_header(
        req,
        PROTOCOL_HEADER_KEY,
        http::header::HeaderValue::from_static(PROTOCOL_HEADER_VALUE),
    );
}

const AUTH_HEADER_KEY: &str = "Authorization";

fn request_insert_auth_header(req: &mut http::Request<()>, credentials: Option<&(Identity, String)>) {
    // TODO: figure out how the token is supposed to be encoded in the request
    if let Some((_, token)) = credentials {
        use base64::Engine;

        let auth_bytes = format!("token:{}", token);
        let encoded = base64::prelude::BASE64_STANDARD.encode(auth_bytes);
        let auth_header_val = format!("Basic {}", encoded);
        request_add_header(
            req,
            AUTH_HEADER_KEY,
            auth_header_val
                .try_into()
                .expect("Failed to convert token to http HeaderValue"),
        )
    };
}

impl WsConnection {
    pub(crate) async fn connect<Host>(
        host: Host,
        db_name: &str,
        credentials: Option<&(Identity, String)>,
        client_address: Address,
        params: WsParams,
    ) -> Result<Self>
    where
        Host: TryInto<Uri>,
        <Host as TryInto<Uri>>::Error: std::error::Error + Send + Sync + 'static,
    {
        let req = make_request(host, db_name, credentials, client_address, params)?;
        let (sock, _): (WebSocketStream<MaybeTlsStream<TcpStream>>, _) = connect_async_with_config(
            req,
            // TODO(kim): In order to be able to replicate module WASM blobs,
            // `cloud-next` cannot have message / frame size limits. That's
            // obviously a bad default for all other clients, though.
            Some(WebSocketConfig {
                max_frame_size: None,
                max_message_size: None,
                ..WebSocketConfig::default()
            }),
            false,
        )
        .await?;
        Ok(WsConnection { sock })
    }

    pub(crate) fn parse_response(bytes: &[u8]) -> Result<ServerMessage<BsatnFormat>> {
        let (compression, bytes) = bytes
            .split_first()
            .ok_or_else(|| anyhow!("Empty raw message. Must have at least a byte for the compression."))?;

        Ok(match *compression {
            SERVER_MSG_COMPRESSION_TAG_NONE => bsatn::from_slice(bytes)?,
            SERVER_MSG_COMPRESSION_TAG_BROTLI => {
                bsatn::from_slice(&brotli_decompress(bytes).context("Failed to Brotli decompress message")?)?
            }
            SERVER_MSG_COMPRESSION_TAG_GZIP => {
                bsatn::from_slice(&gzip_decompress(bytes).context("Failed to gzip decompress message")?)?
            }
            c => bail!("Unknown compression format `{c}`"),
        })
    }

    pub(crate) fn encode_message(msg: ClientMessage<Bytes>) -> WebSocketMessage {
        WebSocketMessage::Binary(bsatn::to_vec(&msg).unwrap())
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
        let mut outgoing_messages = Some(outgoing_messages);
        loop {
            tokio::select! {
                incoming = self.sock.try_next() => match incoming {
                    Err(tokio_tungstenite::tungstenite::error::Error::ConnectionClosed) | Ok(None) => break,

                    Err(e) => Self::maybe_log_error::<(), _>(
                        "Error reading message from read WebSocket stream",
                        Err(e),
                    ),

                    Ok(Some(WebSocketMessage::Binary(bytes))) => {
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

                    Ok(Some(WebSocketMessage::Ping(_))) => {}

                    Ok(Some(other)) => log::warn!("Unexpected WebSocket message {:?}", other),
                },

                // this is stupid. we want to handle the channel close *once*, and then disable this branch
                Some(outgoing) = async { Some(outgoing_messages.as_mut()?.next().await) } => match outgoing {
                    Some(outgoing) => {
                        let msg = Self::encode_message(outgoing);
                        Self::maybe_log_error(
                            "Error sending outgoing message",
                                self.sock.send(msg).await,
                        );
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
