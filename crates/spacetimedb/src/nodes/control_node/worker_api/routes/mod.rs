use super::worker_connection_index::WORKER_CONNECTION_INDEX;
use crate::{
    hash::Hash,
    nodes::control_node::{control_db, controller, object_db},
    websocket,
};
use gotham::handler::HandlerError;
use gotham::prelude::StaticResponseExtender;
use gotham::state::State;
use gotham::state::StateData;
use gotham::{
    prelude::*,
    router::{build_simple_router, Router},
    state::request_id,
};
use hyper::header::AUTHORIZATION;
use hyper::Body;
use hyper::Response;
use hyper::StatusCode;
use lazy_static::lazy_static;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use tokio::spawn;

lazy_static! {
    static ref SEPARATOR: Regex = Regex::new(r"\s*,\s*").unwrap();
}

pub const BIN_PROTOCOL: &str = "v1.bin.spacetimedb-worker-api";

#[derive(Deserialize, StateData, StaticResponseExtender)]
pub struct JoinParams {}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct JoinQueryParams {
    node_id: Option<u64>,
}

async fn join(state: State) -> Result<(State, Response<Body>), (State, HandlerError)> {
    let (mut state, headers, key, on_upgrade, protocol) = websocket::validate_upgrade(state)?;
    let JoinQueryParams { node_id } = JoinQueryParams::take_from(&mut state);

    if BIN_PROTOCOL != protocol {
        log::debug!("Unsupported protocol: {}", protocol);
        return Err((
            state,
            HandlerError::from(anyhow::anyhow!("Unsupported protocol.")).with_status(StatusCode::BAD_REQUEST),
        ));
    };

    let auth_header = headers.get(AUTHORIZATION);
    if let Some(_auth_header) = auth_header {
        // TODO(cloutiertyler): Validate the credentials of this connection
    }

    let node_id = if let Some(node_id) = node_id {
        if let Some(node) = control_db::get_node(node_id).await.unwrap() {
            node.id
        } else {
            controller::create_node().await.unwrap()
        }
    } else {
        controller::create_node().await.unwrap()
    };
    let req_id = request_id(&state).to_owned();

    spawn(async move {
        let ws = websocket::execute_upgrade(&req_id, on_upgrade).await.unwrap();

        let ip_address = headers.get("x-forwarded-for").and_then(|value| {
            value.to_str().ok().and_then(|str| {
                let split = SEPARATOR.split(str);
                let splits: Vec<_> = split.into_iter().collect();
                splits.first().map(|x| *x)
            })
        });

        match ip_address {
            Some(ip) => log::debug!("New worker connected from ip {}", ip),
            None => log::debug!("New worker connected from unknown ip"),
        }

        {
            let wci = &mut WORKER_CONNECTION_INDEX.lock().unwrap();
            wci.new_client(node_id, ws);
        }

        controller::node_connected(node_id).await.unwrap();
    });

    let mut custom_headers = HashMap::new();
    custom_headers.insert("spacetimedb-node-id".to_string(), node_id.to_string());
    Ok((state, websocket::accept_ws_res(&key, &protocol, custom_headers)))
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
pub struct WasmBytesParams {
    wasm_bytes_address: String,
}

async fn wasm_bytes(mut state: State) -> Result<(State, Response<Body>), (State, HandlerError)> {
    let WasmBytesParams { wasm_bytes_address } = WasmBytesParams::take_from(&mut state);

    let hash = match Hash::from_hex(&wasm_bytes_address) {
        Ok(hash) => hash,
        Err(err) => {
            log::debug!("{}", err);
            return Err((
                state,
                HandlerError::from(anyhow::anyhow!("Unable to decode object address."))
                    .with_status(StatusCode::BAD_REQUEST),
            ));
        }
    };
    let wasm_bytes = object_db::get_object(&hash).await.unwrap();

    if let Some(wasm_bytes) = wasm_bytes {
        let res = Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(wasm_bytes))
            .unwrap();
        Ok((state, res))
    } else {
        let res = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
        Ok((state, res))
    }
}

pub fn router() -> Router {
    build_simple_router(|route| {
        route
            .get("/join")
            .with_path_extractor::<JoinParams>()
            .with_query_string_extractor::<JoinQueryParams>()
            .to_async(join);
        route
            .get("/wasm_bytes/:wasm_bytes_address")
            .with_path_extractor::<WasmBytesParams>()
            .to_async(wasm_bytes);
    })
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use gotham::test::TestServer;
    // use hyper::{Body, StatusCode};

    #[test]
    fn init_database() {
        // let test_server = TestServer::new(router()).unwrap();
        // let uri = "http://localhost/database/init/clockworklabs/bitcraft";
        // let body = Body::empty();
        // let mime = "application/octet-stream".parse().unwrap();
        // let response = test_server.client().post(uri, body, mime).perform().unwrap();

        // assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
