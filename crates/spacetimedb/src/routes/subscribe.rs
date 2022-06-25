use crate::api;
use crate::hash::Hash;
use crate::clients::client_connection_index::CLIENT_ACTOR_INDEX;
use crate::wasm_host;
use gotham::handler::HandlerError;
use gotham::prelude::StaticResponseExtender;
use gotham::state::FromState;
use gotham::state::State;
use gotham::state::StateData;
use gotham::state::request_id;
use hyper::Body;
use hyper::HeaderMap;
use hyper::Response;
use hyper::StatusCode;
use hyper::header::AUTHORIZATION;
use hyper::header::CONNECTION;
use hyper::header::HeaderValue;
use hyper::header::SEC_WEBSOCKET_ACCEPT;
use hyper::header::SEC_WEBSOCKET_KEY;
use hyper::header::SEC_WEBSOCKET_PROTOCOL;
use hyper::header::UPGRADE;
use hyper::upgrade::OnUpgrade;
use hyper::upgrade::Upgraded;
use regex::Regex;
use serde::Deserialize;
use sha1::{Sha1, Digest};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::Role;
use lazy_static::lazy_static;
use crate::identity::decode_token;

lazy_static! {
    static ref SEPARATOR: Regex = Regex::new(r"\s*,\s*").unwrap();
}

const PROTO_WEBSOCKET: &str = "websocket";

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
        .header("Spacetime-Identity", hex::encode(identity))
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

fn invalid_token_res() -> Response<Body> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(Body::empty())
        .unwrap()
}

fn meets_protocol_requirements(test_protocol_version: &str) -> bool {
    let target_protocol_version = "20"; // TODO

    let test_components = test_protocol_version.split(".");
    let target_components = target_protocol_version.split(".");

    let mut test_ints: [u32; 3] = [0, 0, 0];
    let mut target_ints: [u32; 3] = [0, 0, 0];

    for (i, component) in test_components.enumerate() {
        if i > test_ints.len() {
            return false;
        }
        let parsed = component.parse::<u32>();
        let parsed = match parsed {
            Ok(value) => value,
            Err(_) => return false,
        };
        test_ints[i] = parsed;
    }

    for (i, component) in target_components.enumerate() {
        if i > target_ints.len() {
            return false;
        }
        let parsed = component.parse::<u32>();
        let parsed = match parsed {
            Ok(value) => value,
            Err(_) => return false,
        };
        target_ints[i] = parsed;
    }

    for i in 0..test_ints.len() {
        if test_ints[i] < target_ints[i] {
            return false;
        }
    }

    return true;
}

async fn on_connected(identity: Hash, module_identity: Hash, module_name: String, headers: HeaderMap, ws: WebSocketStream<Upgraded>) {
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

    let id = {
        let cai = &mut CLIENT_ACTOR_INDEX.lock().unwrap();
        cai.new_client(identity, module_identity, module_name.clone(), ws)
    };

    // Get the right module and add this client as a subscriber
    // TODO: Should maybe even do this before the upgrade and refuse connection
    // TODO: Should also maybe refactor the code and the protocol to allow a single websocket
    // to connect to multiple modules
    let host = wasm_host::get_host();
    let module = host.get_module(module_identity, module_name).await.unwrap(); 
    module.add_subscriber(id).await.unwrap();
}

async fn on_upgrade(mut state: State, headers: HeaderMap, on_upgrade: OnUpgrade) -> Result<(State, Response<Body>), (State, HandlerError)> {
    let key = match headers.get(SEC_WEBSOCKET_KEY).ok_or(()) {
        Ok(value) => value,
        Err(_) => {
            log::debug!("Client did not provide a sec-websocket-key.");
            return Ok((state, bad_request_res()));
        }
    }
    .clone();

    let protocol_versions = headers.get_all(SEC_WEBSOCKET_PROTOCOL);
    let mut count = 0;
    let mut protocol_version: Option<&HeaderValue> = None;
    for version in protocol_versions {
        count += 1;
        protocol_version = Some(version);
    }
    if count != 1 {
        log::debug!("Client tried to connect without protocol version (or provided mulitple).");
        return Ok((state, invalid_protocol_res()));
    }
    let protocol_version_header = protocol_version.unwrap();
    let protocol_version = match protocol_version_header.to_str() {
        Ok(value) => value,
        Err(_) => {
            log::debug!("Could not convert protocol version to string.");
            return Ok((state, invalid_protocol_res()));
        }
    };

    if !meets_protocol_requirements(protocol_version) {
        log::debug!(
            "Client with protocol version {} did not meet the requirement of {}",
            protocol_version, 22
        );
        return Ok((state, invalid_protocol_res()));
    }
    let protocol_version_header = protocol_version_header.clone();

    // Validate the credentials of this connection
    let auth_header = headers.get(AUTHORIZATION);
    let (identity, identity_token) = if let Some(auth_header) = auth_header {

        // Yes, this is using basic auth. See the below issues.
        // The current form is: Authorization: Basic base64("token:<token>")
        // FOOLS, the lot of them!
        // If/when they fix this issue, this should be changed from
        // basic auth, to a `Authorization: Bearer <token>` header
        // https://github.com/whatwg/websockets/issues/16
        // https://github.com/sta/websocket-sharp/pull/22
        
        let auth_header = auth_header.to_str().unwrap_or_default().to_string();
        let encoded_token = auth_header.split("Basic ").collect::<Vec<&str>>().get(1).map(|s| *s);
        let token_string = encoded_token 
            .and_then(|encoded_token| base64::decode(encoded_token).ok())
            .and_then(|token_buf| String::from_utf8(token_buf).ok());
        let token_string = token_string.as_deref();
        let token = match token_string {
            Some(token_str) => {
                let split = token_str.split(":").collect::<Vec<&str>>();
                if split.get(0).map(|s| *s) != Some("token") {
                    None
                } else {
                    split.get(1).map(|s| *s)
                }
            }
            None => None,
        };

        let token_str = if let Some(token) = token {
            token
        } else {
            return Ok((state, invalid_token_res()));
        };

        let token = decode_token(&token_str);
        let token = match token {
            Ok(token) => token,
            Err(error) => {
                log::info!("Deny upgrade. Invalid token: {}", error);
                return Ok((state, invalid_token_res()));
            }
        };

        let hex_identity = token.claims.hex_identity;
        let identity = hex::decode(hex_identity).expect("If this happens we gave out invalid claims.");
        let identity = Hash::from_iter(identity);
        (identity, token_str.to_string())
    } else {
        // Generate a new identity if this connection doesn't have one already
        let (identity, identity_token) = api::spacetime_identity().await.unwrap();
        (identity, identity_token)
    };

    let SubscribeParams {
        identity: hex_module_identity,
        name: module_name,
    } = SubscribeParams::take_from(&mut state);
    let module_identity = match hex::decode(hex_module_identity) {
        Ok(i) => Hash::from_iter(i),
        Err(error) => {
            log::info!("Can't decode {}", error);
            return Ok((state, bad_request_res()))
        }
    };

    let req_id = request_id(&state).to_owned();
    tokio::spawn(async move {
        let ws = match on_upgrade.await {
            Ok(upgraded) => Ok(WebSocketStream::from_raw_socket(upgraded, Role::Server, None).await),
            Err(err) => Err(err),
        };
        match ws {
            Ok(ws) => on_connected(identity, module_identity, module_name, headers, ws).await,
            Err(err) => log::error!("WebSocket init error for req_id {}: {}", req_id, err),
        }
    });

    Ok((state, accept_ws_res(&key, &protocol_version_header, identity, identity_token)))
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
        _ => Ok((
            state,
            Response::new(Body::from(
                r#"
<!DOCTYPE html>
<body>
<h1>Websocket Echo Server</h1>
<form id="ws" onsubmit="return send(this.message);">
    <input name="message">
    <input type="submit" value="Send">
</form>
<script>
    var sock = new WebSocket("ws://" + window.location.host + "/database/subscribe/c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470/test", ["22"]);
    sock.onopen = recv.bind(window, "Connected");
    sock.onclose = function(event) {
        recv("Disconnected");
        console.log(event);
    }
    sock.onmessage = function(msg) { recv(msg.data) };
    sock.onerror = function(err) {
        recv("Error: " + err);
        console.log(err);
    };

    function recv(msg) {
        var e = document.createElement("PRE");
        e.innerText = msg;
        document.body.appendChild(e);
    }

    function send(msg) {
        if (msg.value === "close") {
            sock.close(1000);
            return false;
        }
        sock.send(msg.value);
        msg.value = "";
        return false;
    }

    //setInterval(function () { send({value: "hey"}); }, 1);
</script>
</body>"#,
            )),
        )),
    }
}