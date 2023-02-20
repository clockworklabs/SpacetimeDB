use super::worker_connection_index::WORKER_CONNECTION_INDEX;
use crate::control_node::controller::Controller;
use gotham::handler::HandlerError;
use gotham::handler::SimpleHandlerResult;
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
use serde::Deserialize;
use spacetimedb::{hash::Hash, websocket};
use std::collections::HashMap;
use tokio::spawn;

pub const BIN_PROTOCOL: &str = "v1.bin.spacetimedb-worker-api";

#[derive(Deserialize, StateData, StaticResponseExtender)]
pub struct JoinParams {}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct JoinQueryParams {
    node_id: Option<u64>,
    advertise_addr: String,
}

async fn join(controller: &Controller, state: &mut State) -> SimpleHandlerResult {
    let (headers, key, on_upgrade, protocol) = websocket::validate_upgrade(state)?;
    let JoinQueryParams {
        node_id,
        advertise_addr,
    } = JoinQueryParams::take_from(state);

    if BIN_PROTOCOL != protocol {
        log::debug!("Unsupported protocol: {}", protocol);
        return Err(HandlerError::from(anyhow::anyhow!("Unsupported protocol.")).with_status(StatusCode::BAD_REQUEST));
    };

    let auth_header = headers.get(AUTHORIZATION);
    if let Some(_auth_header) = auth_header {
        // TODO(cloutiertyler): Validate the credentials of this connection
    }

    let node_id = if let Some(node_id) = node_id {
        if let Some(mut node) = controller.control_db.get_node(node_id).await.unwrap() {
            node.advertise_addr = advertise_addr;
            controller.control_db.update_node(node).await.unwrap();
            node_id
        } else {
            controller.create_node(advertise_addr).await.unwrap()
        }
    } else {
        controller.create_node(advertise_addr).await.unwrap()
    };
    let req_id = request_id(&state).to_owned();

    let controller = controller.clone();
    spawn(async move {
        let ws = websocket::execute_upgrade(&req_id, on_upgrade, None).await.unwrap();

        let ip_address = headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| {
                let (first, _) = value.split_once(',')?;
                Some(first.trim())
            });

        match ip_address {
            Some(ip) => log::debug!("New worker connected from ip {}", ip),
            None => log::debug!("New worker connected from unknown ip"),
        }

        {
            let wci = &mut WORKER_CONNECTION_INDEX.lock().unwrap();
            wci.new_client(node_id, ws, controller.clone());
        }

        controller.node_connected(node_id).await.unwrap();
    });

    let mut custom_headers = HashMap::new();
    custom_headers.insert("spacetimedb-node-id".to_string(), node_id.to_string());
    Ok(websocket::accept_ws_res(&key, &protocol, custom_headers))
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
pub struct ProgramBytesParams {
    program_bytes_address: String,
}

async fn program_bytes(controller: &Controller, state: &mut State) -> SimpleHandlerResult {
    let ProgramBytesParams { program_bytes_address } = ProgramBytesParams::take_from(state);

    let hash = match Hash::from_hex(&program_bytes_address) {
        Ok(hash) => hash,
        Err(err) => {
            log::debug!("{}", err);
            return Err(HandlerError::from(anyhow::anyhow!("Unable to decode object address."))
                .with_status(StatusCode::BAD_REQUEST));
        }
    };
    let program_bytes = controller.object_db.get_object(&hash).unwrap();

    if let Some(program_bytes) = program_bytes {
        let res = Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(program_bytes))
            .unwrap();
        Ok(res)
    } else {
        let res = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
        Ok(res)
    }
}

pub fn router(controller: &Controller) -> Router {
    build_simple_router(|route| {
        route
            .get("/join")
            .with_path_extractor::<JoinParams>()
            .with_query_string_extractor::<JoinQueryParams>()
            .to_new_handler(spacetimedb_client_api::with_ctx!(controller, join));
        route
            .get("/program_bytes/:program_bytes_address")
            .with_path_extractor::<ProgramBytesParams>()
            .to_new_handler(spacetimedb_client_api::with_ctx!(controller, program_bytes));
    })
}
