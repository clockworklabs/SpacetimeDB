use super::super::client_connection::Protocol;
use super::super::client_connection_index::CLIENT_ACTOR_INDEX;
use crate::auth::get_creds_from_header;
use crate::auth::invalid_token_res;
use crate::hash::Hash;
use crate::nodes::worker_node::control_node_connection::ControlNodeClient;
use gotham::handler::HandlerError;
use gotham::prelude::StaticResponseExtender;
use gotham::state::request_id;
use gotham::state::FromState;
use gotham::state::State;
use gotham::state::StateData;
use hyper::header::HeaderValue;
use hyper::header::AUTHORIZATION;
use hyper::header::CONNECTION;
use hyper::header::SEC_WEBSOCKET_ACCEPT;
use hyper::header::SEC_WEBSOCKET_KEY;
use hyper::header::SEC_WEBSOCKET_PROTOCOL;
use hyper::header::UPGRADE;
use hyper::upgrade::OnUpgrade;
use hyper::upgrade::Upgraded;
use hyper::Body;
use hyper::HeaderMap;
use hyper::Response;
use hyper::StatusCode;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::WebSocketStream;

lazy_static! {
    static ref SEPARATOR: Regex = Regex::new(r"\s*,\s*").unwrap();
}

const PROTO_WEBSOCKET: &str = "websocket";
const TEXT_PROTOCOL: &str = "v1.text.spacetimedb";
const BIN_PROTOCOL: &str = "v1.bin.spacetimedb";

#[derive(Deserialize, StateData, StaticResponseExtender)]
pub struct SubscribeParams {
    identity: String,
    name: String,
}

fn accept_ws_res(key: &HeaderValue, protocol: &HeaderValue, identity: Hash, identity_token: String) -> Response<Body> {
    fn accept_key(key: &[u8]) -> String {
        const WS_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
        let mut sha1 = Sha1::new();
        sha1.update(key);
        sha1.update(WS_GUID);
        let digest = sha1.finalize();
        base64::encode(digest)
    }

    Response::builder()
        .header(UPGRADE, PROTO_WEBSOCKET)
        .header(CONNECTION, "upgrade")
        .header(SEC_WEBSOCKET_ACCEPT, accept_key(key.as_bytes()))
        .header(SEC_WEBSOCKET_PROTOCOL, protocol)
        .header("Spacetime-Identity", identity.to_hex())
        .header("Spacetime-Identity-Token", identity_token)
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .body(Body::empty())
        .unwrap()
}

fn bad_request_res() -> Response<Body> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(Body::empty())
        .unwrap()
}

fn invalid_protocol_res() -> Response<Body> {
    Response::builder()
        .status(StatusCode::UPGRADE_REQUIRED)
        .header(UPGRADE, PROTO_WEBSOCKET)
        .header(CONNECTION, "upgrade")
        .header(SEC_WEBSOCKET_PROTOCOL, "null")
        .body(Body::empty())
        .unwrap()
}

async fn on_connected(
    identity: Hash,
    identity_token: String,
    module_identity: Hash,
    module_name: String,
    headers: HeaderMap,
    protocol: Protocol,
    ws: WebSocketStream<Upgraded>,
) {
    let ip_address = headers.get("x-forwarded-for").and_then(|value| {
        value.to_str().ok().and_then(|str| {
            let split = SEPARATOR.split(str);
            let splits: Vec<_> = split.into_iter().collect();
            splits.first().map(|x| *x)
        })
    });

    match ip_address {
        Some(ip) => log::debug!("New client connected from ip {}", ip),
        None => log::debug!("New client connected from unknown ip"),
    }

    let sender = {
        let cai = &mut CLIENT_ACTOR_INDEX.lock().unwrap();
        let id = cai.new_client(identity, module_identity, module_name.clone(), protocol, ws);
        cai.get_client(&id).unwrap().sender()
    };

    // Send the client their identity token message as the first message
    // NOTE: We're adding this to the protocol because some client libraries are
    // unable to access the http response headers.
    // Clients that receive the token from the response headers should ignore this
    // message.
    sender.send_identity_token_message(identity, identity_token).await;
}

async fn on_upgrade(
    mut state: State,
    headers: HeaderMap,
    on_upgrade: OnUpgrade,
) -> Result<(State, Response<Body>), (State, HandlerError)> {
    let key = match headers.get(SEC_WEBSOCKET_KEY).ok_or(()) {
        Ok(value) => value,
        Err(_) => {
            log::debug!("Client did not provide a sec-websocket-key.");
            return Ok((state, bad_request_res()));
        }
    }
    .clone();

    let protocols = headers.get_all(SEC_WEBSOCKET_PROTOCOL);
    let mut count = 0;
    let mut protocol: Option<&HeaderValue> = None;
    for p in protocols {
        count += 1;
        protocol = Some(p);
    }
    if count != 1 {
        log::debug!("Client tried to connect without protocol version (or provided mulitple).");
        return Ok((state, invalid_protocol_res()));
    }
    let protocol_header = protocol.unwrap();
    let protocol = match protocol_header.to_str() {
        Ok(value) => value,
        Err(_) => {
            log::debug!("Could not convert protocol version to string.");
            return Ok((state, invalid_protocol_res()));
        }
    };

    let protocol = match protocol {
        TEXT_PROTOCOL => Protocol::Text,
        BIN_PROTOCOL => Protocol::Binary,
        _ => {
            log::debug!("Unsupported protocol: {}", protocol);
            return Ok((state, invalid_protocol_res()));
        }
    };

    let protocol_header = protocol_header.clone();

    let auth_header = headers.get(AUTHORIZATION);
    let (identity, identity_token) = if let Some(auth_header) = auth_header {
        // Validate the credentials of this connection
        match get_creds_from_header(auth_header) {
            Ok(v) => v,
            Err(_) => return Ok((state, invalid_token_res())),
        }
    } else {
        // Generate a new identity if this connection doesn't have one already
        let (identity, identity_token) = ControlNodeClient::get_shared().get_new_identity().await.unwrap();
        (identity, identity_token)
    };

    let SubscribeParams {
        identity: hex_module_identity,
        name: module_name,
    } = SubscribeParams::take_from(&mut state);
    let module_identity = match Hash::from_hex(hex_module_identity.as_str()) {
        Ok(h) => h,
        Err(error) => {
            log::info!("Can't decode {}", error);
            return Ok((state, bad_request_res()));
        }
    };

    let req_id = request_id(&state).to_owned();
    let identity_token_clone = identity_token.clone();
    tokio::spawn(async move {
        let ws = match on_upgrade.await {
            Ok(upgraded) => Ok(WebSocketStream::from_raw_socket(upgraded, Role::Server, None).await),
            Err(err) => Err(err),
        };
        match ws {
            Ok(ws) => on_connected(identity, identity_token_clone, module_identity, module_name, headers, protocol, ws).await,
            Err(err) => log::error!("WebSocket init error for req_id {}: {}", req_id, err),
        }
    });

    Ok((state, accept_ws_res(&key, &protocol_header, identity, identity_token)))
}

pub async fn handle_websocket(mut state: State) -> Result<(State, Response<Body>), (State, HandlerError)> {
    let headers = HeaderMap::take_from(&mut state);
    let on_upgrade_value = OnUpgrade::try_take_from(&mut state);

    /// Check if a WebSocket upgrade was requested.
    fn requested(headers: &HeaderMap) -> bool {
        headers.get(UPGRADE) == Some(&HeaderValue::from_static(PROTO_WEBSOCKET))
    }

    match on_upgrade_value {
        Some(on_upgrade_value) if requested(&headers) => on_upgrade(state, headers, on_upgrade_value).await,
        _ => Err((
            state,
            HandlerError::from(anyhow::anyhow!("Missing upgrade header.")).with_status(StatusCode::BAD_REQUEST),
        )),
    }
}
