use std::collections::HashMap;

use gotham::{
    handler::HandlerError,
    state::{FromState, State},
};
use hyper::{
    header::{HeaderValue, CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_PROTOCOL, UPGRADE},
    upgrade::{OnUpgrade, Upgraded},
    Body, HeaderMap, Response, StatusCode,
};
use sha1::{Digest, Sha1};
use tokio_tungstenite::{tungstenite::protocol::Role, WebSocketStream};

const PROTO_WEBSOCKET: &str = "websocket";

pub fn accept_ws_res(key: &HeaderValue, protocol: &str, custom_headers: HashMap<String, String>) -> Response<Body> {
    fn accept_key(key: &[u8]) -> String {
        const WS_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
        let mut sha1 = Sha1::new();
        sha1.update(key);
        sha1.update(WS_GUID);
        let digest = sha1.finalize();
        base64::encode(digest)
    }

    let mut builder = Response::builder()
        .header(UPGRADE, PROTO_WEBSOCKET)
        .header(CONNECTION, "upgrade")
        .header(SEC_WEBSOCKET_ACCEPT, accept_key(key.as_bytes()))
        .header(SEC_WEBSOCKET_PROTOCOL, protocol);

    for (k, v) in custom_headers {
        builder = builder.header(k, v);
    }

    builder
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .body(Body::empty())
        .unwrap()
}

pub fn validate_upgrade(
    mut state: State,
) -> Result<(State, HeaderMap, HeaderValue, OnUpgrade, String), (State, HandlerError)> {
    let headers = HeaderMap::take_from(&mut state);
    let on_upgrade_value = OnUpgrade::try_take_from(&mut state);

    /// Check if a WebSocket upgrade was requested.
    fn requested(headers: &HeaderMap) -> bool {
        headers.get(UPGRADE) == Some(&HeaderValue::from_static(PROTO_WEBSOCKET))
    }

    let on_upgrade_value = match on_upgrade_value {
        Some(on_upgrade_value) if requested(&headers) => on_upgrade_value,
        _ => {
            return Err((
                state,
                HandlerError::from(anyhow::anyhow!("Missing upgrade header.")).with_status(StatusCode::BAD_REQUEST),
            ))
        }
    };

    let key = match headers.get(SEC_WEBSOCKET_KEY).ok_or(()) {
        Ok(value) => value,
        Err(_) => {
            log::debug!("Client did not provide a sec-websocket-key.");
            return Err((
                state,
                HandlerError::from(anyhow::anyhow!("Missing sec-websocket-key.")).with_status(StatusCode::BAD_REQUEST),
            ));
        }
    }
    .clone();

    let protocols = headers.get_all(SEC_WEBSOCKET_PROTOCOL);
    let mut count = 0;
    let mut protocol_header: Option<&HeaderValue> = None;
    for p in protocols {
        count += 1;
        protocol_header = Some(p);
    }
    if count != 1 {
        log::debug!("Client tried to connect without protocol version (or provided mulitple).");
        return Err((
            state,
            HandlerError::from(anyhow::anyhow!("Invalid protocol.")).with_status(StatusCode::UPGRADE_REQUIRED),
        ));
    }
    let protocol_header = protocol_header.unwrap().clone();
    let protocol = match protocol_header.to_str() {
        Ok(value) => value,
        Err(_) => {
            log::debug!("Could not convert protocol version to string.");
            return Err((
                state,
                HandlerError::from(anyhow::anyhow!("Malformed protocol.")).with_status(StatusCode::BAD_REQUEST),
            ));
        }
    };

    Ok((state, headers, key, on_upgrade_value, protocol.to_string()))
}

pub async fn execute_upgrade(req_id: &str, on_upgrade: OnUpgrade) -> Result<WebSocketStream<Upgraded>, anyhow::Error> {
    let ws = match on_upgrade.await {
        Ok(upgraded) => WebSocketStream::from_raw_socket(upgraded, Role::Server, None).await,
        Err(err) => {
            log::error!("WebSocket init error for req_id {}: {}", req_id, err);
            return Err(anyhow::anyhow!("Upgrade failed."));
        }
    };
    Ok(ws)
}
