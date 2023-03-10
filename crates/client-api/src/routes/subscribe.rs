use std::collections::HashMap;

use anyhow::Context;
use gotham::handler::SimpleHandlerResult;
use gotham::prelude::MapHandlerError;
use gotham::prelude::StaticResponseExtender;
use gotham::state::request_id;
use gotham::state::FromState;
use gotham::state::State;
use gotham::state::StateData;
use hyper::header::AUTHORIZATION;
use hyper::header::CONNECTION;
use hyper::header::SEC_WEBSOCKET_PROTOCOL;
use hyper::header::UPGRADE;
use hyper::upgrade::Upgraded;
use hyper::Body;
use hyper::HeaderMap;
use hyper::Response;
use hyper::StatusCode;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use spacetimedb::address::Address;
use spacetimedb::auth::get_creds_from_header;
use spacetimedb::auth::identity::encode_token;
use spacetimedb::auth::invalid_token_res;
use spacetimedb::client::client_connection::Protocol;
use spacetimedb::client::client_connection_index::ModuleHost;
use spacetimedb::client::client_connection_index::NoSuchModule;
use spacetimedb::client::client_connection_index::CLIENT_ACTOR_INDEX;
use spacetimedb::control_db::CONTROL_DB;
use spacetimedb::host::host_controller;
use spacetimedb::identity::Identity;
use spacetimedb::websocket;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::WebSocketStream;

lazy_static! {
    static ref SEPARATOR: Regex = Regex::new(r"\s*,\s*").unwrap();
}

const PROTO_WEBSOCKET: &str = "websocket";
const TEXT_PROTOCOL: &str = "v1.text.spacetimedb";
const BIN_PROTOCOL: &str = "v1.bin.spacetimedb";

#[derive(Deserialize, StateData, StaticResponseExtender)]
pub struct SubscribeQueryParams {
    name_or_address: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
pub struct SubscribeParams {}

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

#[allow(clippy::too_many_arguments)]
async fn on_connected(
    identity: Identity,
    identity_token: String,
    target_address: Address,
    headers: HeaderMap,
    protocol: Protocol,
    ws: WebSocketStream<Upgraded>,
    instance_id: u64,
    module: ModuleHost,
) -> Result<(), NoSuchModule> {
    let ip_address = headers.get("x-forwarded-for").and_then(|value| {
        value.to_str().ok().and_then(|str| {
            let split = SEPARATOR.split(str);
            let splits: Vec<_> = split.into_iter().collect();
            splits.first().copied()
        })
    });

    match ip_address {
        Some(ip) => log::debug!("New client connected from ip {}", ip),
        None => log::debug!("New client connected from unknown ip"),
    }

    let (_, sender) = CLIENT_ACTOR_INDEX
        .new_client(identity, target_address, protocol, ws, instance_id, module)
        .await?;

    // Send the client their identity token message as the first message
    // NOTE: We're adding this to the protocol because some client libraries are
    // unable to access the http response headers.
    // Clients that receive the token from the response headers should ignore this
    // message.
    sender.send_identity_token_message(identity, identity_token).await;
    Ok(())
}

pub async fn handle_websocket(state: &mut State) -> SimpleHandlerResult {
    let (headers, key, on_upgrade, protocol_string) = websocket::validate_upgrade(state)?;
    let protocol = match protocol_string.as_str() {
        TEXT_PROTOCOL => Protocol::Text,
        BIN_PROTOCOL => Protocol::Binary,
        _ => {
            return Ok(invalid_protocol_res());
        }
    };

    let auth_header = headers.get(AUTHORIZATION);
    let (identity, identity_token) = if let Some(auth_header) = auth_header {
        // Validate the credentials of this connection
        match get_creds_from_header(auth_header) {
            Ok(v) => v,
            Err(_) => return Ok(invalid_token_res()),
        }
    } else {
        // Generate a new identity if this connection doesn't have one already
        let identity = CONTROL_DB.alloc_spacetime_identity().await.unwrap();
        let identity_token = encode_token(identity).unwrap();
        (identity, identity_token)
    };

    let SubscribeParams {} = SubscribeParams::take_from(state);
    let SubscribeQueryParams { name_or_address } = SubscribeQueryParams::take_from(state);
    let target_address = if let Ok(address) = Address::from_hex(&name_or_address) {
        address
    } else if let Some(address) = CONTROL_DB.spacetime_dns(&name_or_address).await.unwrap() {
        address
    } else {
        return Ok(bad_request_res());
    };

    // TODO: Should also maybe refactor the code and the protocol to allow a single websocket
    // to connect to multiple modules
    let database = CONTROL_DB.get_database_by_address(&target_address).await.unwrap();
    let Some(database) = database else {
        return Ok(bad_request_res());
    };
    let database_instance = CONTROL_DB.get_leader_database_instance_by_database(database.id).await;
    let Some(database_instance) = database_instance else {
        return Ok(bad_request_res());
    };
    let instance_id = database_instance.id;

    let req_id = request_id(state).to_owned();
    let identity_token_clone = identity_token.clone();

    let host = host_controller::get_host();
    let module = host
        .get_module(instance_id)
        .context("Database instance not scheduled to this node yet.")
        .map_err_with_status(StatusCode::NOT_FOUND)?;

    tokio::spawn(async move {
        let config = WebSocketConfig {
            max_send_queue: None,
            max_message_size: Some(0x2000000),
            max_frame_size: None,
            accept_unmasked_frames: false,
        };
        let ws = websocket::execute_upgrade(&req_id, on_upgrade, Some(config))
            .await
            .unwrap();

        if let Err(err) = on_connected(
            identity,
            identity_token_clone,
            target_address,
            headers,
            protocol,
            ws,
            instance_id,
            module,
        )
        .await
        {
            // debug here should be fine, because `on_connected` can only
            // error out on `NoSuchModule` and we just found a module, so
            // this should be really rare
            log::debug!("Error during client connection: {:?}", err);
        }
    });

    let mut custom_headers = HashMap::new();
    custom_headers.insert("Spacetime-Identity".to_string(), identity.to_hex());
    custom_headers.insert("Spacetime-Identity-Token".to_string(), identity_token);
    Ok(websocket::accept_ws_res(&key, &protocol_string, custom_headers))
}
