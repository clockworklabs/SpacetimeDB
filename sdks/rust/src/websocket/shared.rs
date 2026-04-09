use bytes::Bytes;
use http::uri::{InvalidUri, Scheme, Uri};
use spacetimedb_client_api_messages::websocket as ws;
use spacetimedb_lib::{bsatn, ConnectionId};
use std::sync::Arc;
use thiserror::Error;
#[cfg(not(feature = "browser"))]
use tokio::net::TcpStream;
#[cfg(not(feature = "browser"))]
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
#[cfg(feature = "browser")]
use tokio_tungstenite_wasm::WebSocketStream;

#[cfg(not(feature = "browser"))]
type TokioTungsteniteError = tokio_tungstenite::tungstenite::Error;
#[cfg(feature = "browser")]
type TokioTungsteniteError = tokio_tungstenite_wasm::Error;

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
        source: Arc<TokioTungsteniteError>,
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

    #[cfg(feature = "browser")]
    #[error("Token verification error: {0}")]
    TokenVerification(String),
}

pub(crate) struct WsConnection {
    pub(super) db_name: Box<str>,
    #[cfg(not(feature = "browser"))]
    pub(super) protocol: super::native::NegotiatedWsProtocol,
    #[cfg(not(feature = "browser"))]
    pub(super) sock: WebSocketStream<MaybeTlsStream<TcpStream>>,
    #[cfg(feature = "browser")]
    pub(super) sock: WebSocketStream,
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
    pub compression: ws::common::Compression,
    /// `Some(true)` to enable confirmed reads for the connection,
    /// `Some(false)` to disable them.
    /// `None` to not set the parameter and let the server choose.
    pub confirmed: Option<bool>,
}

pub(super) fn make_uri_impl(
    host: Uri,
    db_name: &str,
    connection_id: Option<ConnectionId>,
    params: WsParams,
    token: Option<&str>,
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
        ws::common::Compression::None => path.push_str("?compression=None"),
        ws::common::Compression::Gzip => path.push_str("?compression=Gzip"),
        // The host uses the same default as the sdk,
        // but in case this changes, we prefer to be explicit now.
        ws::common::Compression::Brotli => path.push_str("?compression=Brotli"),
    };

    // Provide the connection ID if the client provided one.
    if let Some(cid) = connection_id {
        // If a connection ID is provided, append it to the path.
        path.push_str("&connection_id=");
        path.push_str(&cid.to_hex());
    }

    // Enable confirmed reads if requested.
    if let Some(confirmed) = params.confirmed {
        path.push_str("&confirmed=");
        path.push_str(if confirmed { "true" } else { "false" });
    }

    // Specify the `token` param if needed
    if let Some(token) = token {
        path.push_str(&format!("&token={token}"));
    }

    parts.path_and_query = Some(path.parse().map_err(|source: InvalidUri| UriError::InvalidUri {
        source: Arc::new(source),
    })?);
    Uri::from_parts(parts).map_err(|source| UriError::InvalidUriParts {
        source: Arc::new(source),
    })
}

/// Decodes one logical v2 server message from an already-decompressed payload.
pub(super) fn decode_v2_server_message(bytes: &[u8]) -> Result<ws::v2::ServerMessage, WsError> {
    bsatn::from_slice(bytes).map_err(|source| WsError::DeserializeMessage { source })
}

/// Encodes one logical v2 client message into raw BSATN bytes.
pub(super) fn encode_v2_client_message_bytes(msg: &ws::v2::ClientMessage) -> Bytes {
    Bytes::from(bsatn::to_vec(msg).expect("should be able to bsatn encode v2 client message"))
}
