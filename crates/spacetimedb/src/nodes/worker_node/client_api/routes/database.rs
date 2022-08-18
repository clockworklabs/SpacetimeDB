use crate::auth::get_creds_from_header;
use crate::auth::invalid_token_res;
use crate::hash::Hash;
use crate::nodes::worker_node::control_node_connection::ControlNodeClient;
use crate::nodes::worker_node::database_logger::DatabaseLogger;
use crate::nodes::worker_node::wasm_host_controller;
use crate::nodes::worker_node::worker_db;
use gotham::anyhow::anyhow;
use gotham::handler::HandlerError;
use gotham::handler::SimpleHandlerResult;
use gotham::prelude::FromState;
use gotham::prelude::StaticResponseExtender;
use gotham::router::builder::*;
use gotham::router::Router;
use gotham::state::State;
use gotham::state::StateData;
use hyper::body::HttpBody;
use hyper::header::AUTHORIZATION;
use hyper::Body;
use hyper::HeaderMap;
use hyper::{Response, StatusCode};
use serde::Deserialize;

use super::subscribe::handle_websocket;
use super::subscribe::SubscribeParams;

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct InitDatabaseParams {
    identity: String,
    name: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct InitDatabaseQueryParams {
    force: Option<bool>,
}

async fn init_database(state: &mut State) -> SimpleHandlerResult {
    let InitDatabaseParams { identity, name } = InitDatabaseParams::take_from(state);
    let InitDatabaseQueryParams { force } = InitDatabaseQueryParams::take_from(state);
    let force = force.unwrap_or(false);
    let body = state.borrow_mut::<Body>();
    let data = hyper::body::to_bytes(body).await;
    let data = match data {
        Ok(data) => data,
        Err(_) => return Err(HandlerError::from(anyhow!("Invalid request body")).with_status(StatusCode::BAD_REQUEST)),
    };
    let identity = match Hash::from_hex(&identity) {
        Ok(identity) => identity,
        Err(e) => {
            return Err(HandlerError::from(e));
        }
    };
    let wasm_bytes = data.to_vec();

    ControlNodeClient::get_shared().init_database(&identity, &name, wasm_bytes, force).await;

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();

    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct UpdateDatabaseParams {
    identity: String,
    name: String,
}

async fn update_module(state: &mut State) -> SimpleHandlerResult {
    let UpdateDatabaseParams { identity, name } = UpdateDatabaseParams::take_from(state);
    let body = state.borrow_mut::<Body>();
    let data = hyper::body::to_bytes(body).await;
    let data = match data {
        Ok(data) => data,
        Err(_) => return Err(HandlerError::from(anyhow!("Invalid request body")).with_status(StatusCode::BAD_REQUEST)),
    };
    let wasm_bytes = data.to_vec();
    let identity = match Hash::from_hex(&identity) {
        Ok(identity) => identity,
        Err(e) => {
            return Err(HandlerError::from(e));
        }
    };

    ControlNodeClient::get_shared().update_database(&identity, &name, wasm_bytes).await;

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();

    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct DeleteDatabaseParams {
    identity: String,
    name: String,
}

async fn delete_module(state: &mut State) -> SimpleHandlerResult {
    let DeleteDatabaseParams { identity, name } = DeleteDatabaseParams::take_from(state);
    let identity = match Hash::from_hex(&identity) {
        Ok(identity) => identity,
        Err(e) => {
            return Err(HandlerError::from(e));
        }
    };

    ControlNodeClient::get_shared().delete_database(&identity, &name).await;

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();
    Ok(res)
}


#[derive(Deserialize, StateData, StaticResponseExtender)]
struct CallParams {
    identity: String,
    name: String,
    reducer: String,
}

async fn call(state: &mut State) -> SimpleHandlerResult {
    let CallParams {
        identity,
        name,
        reducer,
    } = CallParams::take_from(state);
    let headers = state.borrow::<HeaderMap>();
    let auth_header = headers.get(AUTHORIZATION);
    let (caller_identity, caller_identity_token) = if let Some(auth_header) = auth_header {
        // Validate the credentials of this connection
        match get_creds_from_header(auth_header) {
            Ok(v) => v,
            Err(_) => return Ok(invalid_token_res()),
        }
    } else {
        // Generate a new identity if this request doesn't have one already
        let (identity, identity_token) = ControlNodeClient::get_shared().get_new_identity().await.unwrap();
        (identity, identity_token)
    };

    let body = state.borrow_mut::<Body>();
    let data = body.data().await;
    if data.is_none() {
        return Err(HandlerError::from(anyhow!("Missing request body.")).with_status(StatusCode::BAD_REQUEST));
    }
    let data = data.unwrap();
    let arg_bytes = data.unwrap().to_vec();

    let identity = Hash::from_hex(&identity).expect("that the client passed a valid hex identity lol");

    for database in worker_db::_get_databases() {
        log::debug!("{:?}", database);
    }

    for instance in worker_db::get_database_instances() {
        log::debug!("{:?}", instance);
    }

    let database = match worker_db::get_database_by_address(&identity, &name) {
        Some(database) => database,
        None => return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND)),
    };
    let database_instance = match worker_db::get_leader_database_instance_by_database(database.id) {
        Some(database) => database,
        None => return Err(HandlerError::from(anyhow!("Database instance not scheduled to this node yet.")).with_status(StatusCode::NOT_FOUND)),
    };
    let instance_id = database_instance.id;
    let host = wasm_host_controller::get_host();

    match host.call_reducer(instance_id, caller_identity, &reducer, arg_bytes).await {
        Ok(_) => {}
        Err(e) => {
            log::error!("{}", e);
            return Err(HandlerError::from(anyhow!("Database instance not ready.")).with_status(StatusCode::SERVICE_UNAVAILABLE));
        }
    }

    let res = Response::builder()
        .header("Spacetime-Identity", caller_identity.to_hex())
        .header("Spacetime-Identity-Token", caller_identity_token)
        .status(StatusCode::OK)
        .body(Body::empty())
        .unwrap();
    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct LogsParams {
    identity: String,
    name: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct LogsQuery {
    num_lines: u32,
}

async fn logs(state: &mut State) -> SimpleHandlerResult {
    let LogsParams { identity, name } = LogsParams::take_from(state);
    let LogsQuery { num_lines } = LogsQuery::take_from(state);

    let identity = Hash::from_hex(&identity).expect("that the client passed a valid hex identity lol");

    let database = match worker_db::get_database_by_address(&identity, &name) {
        Some(database) => database,
        None => return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND)),
    };
    let database_instance = worker_db::get_leader_database_instance_by_database(database.id);
    let instance_id = database_instance.unwrap().id;

    let filepath = DatabaseLogger::filepath(&identity, &name, instance_id);
    let lines = DatabaseLogger::read_latest(&filepath, num_lines).await;

    let res = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(lines))
        .unwrap();

    Ok(res)
}

pub fn router() -> Router {
    build_simple_router(|route| {
        route
            .post("/:identity/:name/init")
            .with_path_extractor::<InitDatabaseParams>()
            .with_query_string_extractor::<InitDatabaseQueryParams>()
            .to_async_borrowing(init_database);

        route
            .post("/:identity/:name/update")
            .with_path_extractor::<UpdateDatabaseParams>()
            .to_async_borrowing(update_module);

        route
            .post("/:identity/:name/delete")
            .with_path_extractor::<DeleteDatabaseParams>()
            .to_async_borrowing(delete_module);

        route
            .get("/:identity/:name/subscribe")
            .with_path_extractor::<SubscribeParams>()
            .to_async(handle_websocket);

        route
            .post("/:identity/:name/call/:reducer")
            .with_path_extractor::<CallParams>()
            .to_async_borrowing(call);

        route
            .get("/:identity/:name/logs")
            .with_path_extractor::<LogsParams>()
            .with_query_string_extractor::<LogsQuery>()
            .to_async_borrowing(logs);
    })
}
