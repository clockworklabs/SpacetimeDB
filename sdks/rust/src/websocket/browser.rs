use super::shared::{
    decode_v2_server_message, encode_v2_client_message_bytes, make_uri_impl, WsConnection, WsError, WsParams,
};
use futures::{SinkExt, StreamExt as _};
use futures_channel::mpsc;
use http::uri::Uri;
use spacetimedb_client_api_messages::websocket as ws;
use spacetimedb_lib::ConnectionId;
use std::sync::Arc;
use tokio_tungstenite_wasm::Message as WebSocketMessage;

use crate::compression::decompress_server_message;
use crate::metrics::CLIENT_METRICS;

fn make_uri(
    host: Uri,
    db_name: &str,
    connection_id: Option<ConnectionId>,
    params: WsParams,
    token: Option<&str>,
) -> Result<Uri, WsError> {
    make_uri_impl(host, db_name, connection_id, params, token).map_err(Into::into)
}

async fn fetch_ws_token(host: &Uri, auth_token: &str) -> Result<String, WsError> {
    use gloo_net::http::{Method, RequestBuilder};
    use js_sys::{Reflect, JSON};
    use wasm_bindgen::{JsCast, JsValue};

    let url = format!("{host}v1/identity/websocket-token");

    // helpers to convert gloo_net::Error or JsValue into WsError::TokenVerification
    let gloo_to_ws_err = |e: gloo_net::Error| match e {
        gloo_net::Error::JsError(js_err) => WsError::TokenVerification(js_err.message),
        gloo_net::Error::SerdeError(e) => WsError::TokenVerification(e.to_string()),
        gloo_net::Error::GlooError(msg) => WsError::TokenVerification(msg),
    };
    let js_to_ws_err = |e: JsValue| {
        if let Some(err) = e.dyn_ref::<js_sys::Error>() {
            WsError::TokenVerification(err.message().into())
        } else if let Some(s) = e.as_string() {
            WsError::TokenVerification(s)
        } else {
            WsError::TokenVerification(format!("{e:?}"))
        }
    };

    let res = RequestBuilder::new(&url)
        .method(Method::POST)
        .header("Authorization", &format!("Bearer {auth_token}"))
        .send()
        .await
        .map_err(gloo_to_ws_err)?;

    if !res.ok() {
        return Err(WsError::TokenVerification(format!(
            "HTTP error: {} {}",
            res.status(),
            res.status_text()
        )));
    }

    let body = res.text().await.map_err(gloo_to_ws_err)?;
    let json = JSON::parse(&body).map_err(js_to_ws_err)?;
    let token_js = Reflect::get(&json, &JsValue::from_str("token")).map_err(js_to_ws_err)?;
    token_js
        .as_string()
        .ok_or_else(|| WsError::TokenVerification("`token` parsing failed".into()))
}

impl WsConnection {
    pub(crate) async fn connect(
        host: Uri,
        db_name: &str,
        token: Option<&str>,
        connection_id: Option<ConnectionId>,
        params: WsParams,
    ) -> Result<Self, WsError> {
        let token = if let Some(auth_token) = token {
            Some(fetch_ws_token(&host, auth_token).await?)
        } else {
            None
        };

        let uri = make_uri(host, db_name, connection_id, params, token.as_deref())?;
        // Browser targets stay on v2 for now. `tokio-tungstenite-wasm` does not
        // expose the negotiated subprotocol, so we cannot safely offer v3 with
        // a real v2 fallback here without replacing the wrapper entirely.
        let sock = tokio_tungstenite_wasm::connect_with_protocols(&uri.to_string(), &[ws::v2::BIN_PROTOCOL])
            .await
            .map_err(|source| WsError::Tungstenite {
                uri,
                source: Arc::new(source),
            })?;

        Ok(WsConnection {
            db_name: db_name.into(),
            sock,
        })
    }

    /// Parses one browser websocket payload, which always uses legacy v2
    /// framing.
    fn parse_v2_response(bytes: &[u8]) -> Result<ws::v2::ServerMessage, WsError> {
        let bytes = &*decompress_server_message(bytes)?;
        decode_v2_server_message(bytes)
    }

    pub(crate) fn spawn_message_loop(
        self,
    ) -> (
        mpsc::UnboundedReceiver<ws::v2::ServerMessage>,
        mpsc::UnboundedSender<ws::v2::ClientMessage>,
    ) {
        let websocket_received = CLIENT_METRICS.websocket_received.with_label_values(&self.db_name);
        let websocket_received_msg_size = CLIENT_METRICS
            .websocket_received_msg_size
            .with_label_values(&self.db_name);
        let record_metrics = move |msg_size: usize| {
            websocket_received.inc();
            websocket_received_msg_size.observe(msg_size as f64);
        };

        let (outgoing_tx, outgoing_rx) = mpsc::unbounded::<ws::v2::ClientMessage>();
        let (incoming_tx, incoming_rx) = mpsc::unbounded::<ws::v2::ServerMessage>();
        let (mut ws_writer, ws_reader) = self.sock.split();

        wasm_bindgen_futures::spawn_local(async move {
            let mut incoming = ws_reader.fuse();
            let mut outgoing = outgoing_rx.fuse();

            loop {
                futures::select! {
                    // 1) inbound WS frames
                    inbound = incoming.next() => match inbound {
                        Some(Err(tokio_tungstenite_wasm::Error::ConnectionClosed)) | None => {
                            gloo_console::log!("Connection closed");
                            break;
                        },

                        Some(Ok(WebSocketMessage::Binary(bytes))) => {
                            record_metrics(bytes.len());
                            match Self::parse_v2_response(&bytes) {
                                Ok(msg) => if let Err(_e) = incoming_tx.unbounded_send(msg) {
                                    gloo_console::warn!("Incoming receiver dropped.");
                                    break;
                                },
                                Err(e) => {
                                    gloo_console::warn!(
                                        "Error decoding WebSocketMessage::Binary payload: ",
                                        format!("{:?}", e)
                                    );
                                }
                            }
                        },

                        Some(Ok(WebSocketMessage::Close(r))) => {
                            let reason: String = if let Some(r) = r {
                                format!("{}:{:?}", r, r.code)
                            } else {String::default()};
                            gloo_console::warn!("Connection Closed.", reason);
                            let _ = ws_writer.close().await;
                            break;
                        },

                        Some(Err(e)) => {
                            gloo_console::warn!(
                                "Error reading message from read WebSocket stream: ",
                                format!("{:?}",e)
                            );
                            break;
                        },

                        Some(Ok(other)) => {
                            record_metrics(other.len());
                            gloo_console::warn!("Unexpected WebSocket message: ", format!("{:?}",other));
                        },
                    },

                    // 2) outbound messages
                    outbound = outgoing.next() => if let Some(client_msg) = outbound {
                        let raw = WebSocketMessage::Binary(encode_v2_client_message_bytes(&client_msg));
                        if let Err(e) = ws_writer.send(raw).await {
                            gloo_console::warn!("Error sending outgoing message:", format!("{:?}",e));
                            break;
                        }
                    } else {
                        // channel closed, so we're done  sending
                        if let Err(e) = ws_writer.close().await {
                            gloo_console::warn!("Error sending close frame:", format!("{:?}", e));
                        }
                        break;
                    },
                }
            }
        });

        (incoming_rx, outgoing_tx)
    }
}
