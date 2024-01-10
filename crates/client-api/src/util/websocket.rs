//! A more flexible version of axum::extract::ws. This could probably get pulled out into its own crate at some point.

use axum::extract::FromRequestParts;
use axum::response::{IntoResponse, Response};
use axum_extra::TypedHeader;
use headers::{Connection, HeaderMapExt, SecWebsocketAccept, SecWebsocketKey, SecWebsocketVersion, Upgrade};
use http::{HeaderName, HeaderValue, Method, StatusCode};
use hyper::upgrade::{OnUpgrade, Upgraded};
use hyper_util::rt::TokioIo;

use super::flat_csv::FlatCsv;

pub use tokio_tungstenite::tungstenite;
pub use tungstenite::protocol::{frame::coding::CloseCode, CloseFrame, Message, WebSocketConfig};

pub type WebSocketStream = tokio_tungstenite::WebSocketStream<TokioIo<Upgraded>>;

pub struct RequestSecWebsocketProtocol(FlatCsv);

impl headers::Header for RequestSecWebsocketProtocol {
    fn name() -> &'static HeaderName {
        &http::header::SEC_WEBSOCKET_PROTOCOL
    }
    fn decode<'i, I: Iterator<Item = &'i HeaderValue>>(values: &mut I) -> Result<Self, headers::Error> {
        Ok(Self(values.collect()))
    }
    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend([self.0.value.clone()])
    }
}

impl RequestSecWebsocketProtocol {
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter()
    }

    pub fn select<S, P>(&self, protocols: impl IntoIterator<Item = (S, P)>) -> Option<(ResponseSecWebsocketProtocol, P)>
    where
        S: for<'a> PartialEq<&'a str> + TryInto<HeaderValue>,
    {
        protocols
            .into_iter()
            .find(|(protoname, _)| self.iter().any(|x| *protoname == x))
            .map(|(protoname, proto)| {
                let proto_header = protoname.try_into().unwrap_or_else(|_| unreachable!());
                (ResponseSecWebsocketProtocol(proto_header), proto)
            })
    }
}

pub struct ResponseSecWebsocketProtocol(pub HeaderValue);

impl headers::Header for ResponseSecWebsocketProtocol {
    fn name() -> &'static HeaderName {
        &http::header::SEC_WEBSOCKET_PROTOCOL
    }
    fn decode<'i, I: Iterator<Item = &'i HeaderValue>>(values: &mut I) -> Result<Self, headers::Error> {
        values.next().cloned().map(Self).ok_or_else(headers::Error::invalid)
    }
    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend([self.0.clone()])
    }
}

pub struct WebSocketUpgrade {
    key: SecWebsocketKey,
    requested_protocol: Option<RequestSecWebsocketProtocol>,
    upgrade: OnUpgrade,
}

pub enum WebSocketUpgradeRejection {
    MethodNotGet,
    BadUpgrade,
    BadVersion,
    KeyMissing,
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for WebSocketUpgrade {
    type Rejection = WebSocketUpgradeRejection;
    async fn from_request_parts(parts: &mut http::request::Parts, _state: &S) -> Result<Self, Self::Rejection> {
        use WebSocketUpgradeRejection::*;

        if parts.method != Method::GET {
            return Err(MethodNotGet);
        }

        let upgrade = parts
            .extensions
            .remove::<OnUpgrade>()
            .filter(|_| {
                parts
                    .headers
                    .typed_get::<Connection>()
                    .map_or(false, |conn| conn.contains("upgrade"))
                    && parts.headers.typed_get::<Upgrade>() == Some(Upgrade::websocket())
            })
            .ok_or(BadUpgrade)?;

        if parts.headers.typed_get::<SecWebsocketVersion>() != Some(SecWebsocketVersion::V13) {
            return Err(BadVersion);
        }

        let key = parts.headers.typed_get::<SecWebsocketKey>().ok_or(KeyMissing)?;

        let requested_protocol = parts.headers.typed_get::<RequestSecWebsocketProtocol>();

        Ok(WebSocketUpgrade {
            key,
            requested_protocol,
            upgrade,
        })
    }
}

impl IntoResponse for WebSocketUpgradeRejection {
    fn into_response(self) -> Response {
        match self {
            Self::MethodNotGet => (StatusCode::METHOD_NOT_ALLOWED, "Request method must be `GET`").into_response(),
            Self::BadUpgrade => (
                StatusCode::UPGRADE_REQUIRED,
                TypedHeader(Connection::upgrade()),
                TypedHeader(Upgrade::websocket()),
                "This service requires use of the websocket protocol",
            )
                .into_response(),
            Self::BadVersion => (
                StatusCode::BAD_REQUEST,
                "`Sec-WebSocket-Version` header did not include '13'",
            )
                .into_response(),
            Self::KeyMissing => (StatusCode::BAD_REQUEST, "`Sec-WebSocket-Key` header missing").into_response(),
        }
    }
}

impl WebSocketUpgrade {
    #[inline]
    pub fn protocol(&self) -> Option<&RequestSecWebsocketProtocol> {
        self.requested_protocol.as_ref()
    }

    /// Select a subprotocol from the ones provided, and prepare a response for the client.
    pub fn select_protocol<S, P>(
        self,
        protocols: impl IntoIterator<Item = (S, P)>,
    ) -> (WebSocketResponse, PendingWebSocket, Option<P>)
    where
        S: for<'a> PartialEq<&'a str> + TryInto<HeaderValue>,
    {
        let (proto_header, proto) = self
            .requested_protocol
            .as_ref()
            .and_then(|proto| proto.select(protocols))
            .unzip();
        let (resp, ws) = self.into_response(proto_header);
        (resp, ws, proto)
    }

    /// Prepare a response with no subprotocol selected.
    #[inline]
    pub fn ignore_protocol(self) -> (WebSocketResponse, PendingWebSocket) {
        self.into_response(None)
    }

    /// Prepare a response with the given subprotocol.
    #[inline]
    pub fn into_response(
        self,
        protocol: Option<ResponseSecWebsocketProtocol>,
    ) -> (WebSocketResponse, PendingWebSocket) {
        let resp = WebSocketResponse {
            accept: self.key.into(),
            protocol,
        };
        (resp, PendingWebSocket(self.upgrade))
    }
}

pub struct PendingWebSocket(OnUpgrade);

impl PendingWebSocket {
    #[inline]
    pub async fn upgrade(self, config: WebSocketConfig) -> hyper::Result<WebSocketStream> {
        let stream = TokioIo::new(self.0.await?);
        Ok(WebSocketStream::from_raw_socket(stream, tungstenite::protocol::Role::Server, Some(config)).await)
    }

    #[inline]
    pub fn into_inner(self) -> OnUpgrade {
        self.0
    }
}

/// An type representing an http response for a successful websocket upgrade. Note that this response
/// must be returned to the client for [`PendingWebSocket::upgrade`] to succeed.
pub struct WebSocketResponse {
    accept: SecWebsocketAccept,
    protocol: Option<ResponseSecWebsocketProtocol>,
}

impl IntoResponse for WebSocketResponse {
    #[inline]
    fn into_response(self) -> Response {
        (
            StatusCode::SWITCHING_PROTOCOLS,
            TypedHeader(Connection::upgrade()),
            TypedHeader(Upgrade::websocket()),
            TypedHeader(self.accept),
            self.protocol.map(TypedHeader),
            (),
        )
            .into_response()
    }
}
