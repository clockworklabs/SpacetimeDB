use crate::nodes::control_node::controller;
use crate::hash::Hash;
use crate::hash::hash_bytes;
use crate::nodes::control_node::object_db;
use gotham::anyhow::anyhow;
use gotham::handler::HandlerError;
use gotham::handler::SimpleHandlerResult;
use gotham::prelude::FromState;
use gotham::prelude::StaticResponseExtender;
use gotham::router::builder::*;
use gotham::router::Router;
use gotham::state::State;
use gotham::state::StateData;
use hyper::Body;
use hyper::{Response, StatusCode};
use serde::Deserialize;

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
    let wasm_bytes = data.to_vec();
    let wasm_bytes_address = hash_bytes(&wasm_bytes);
    object_db::insert_object(wasm_bytes).await.unwrap();

    let identity = Hash::from_hex(&identity).unwrap();

    let num_replicas = 1;

    if let Err(err) = controller::insert_database(&identity, &name, &wasm_bytes_address, num_replicas, force).await {
        log::debug!("{err}");
        return Err(HandlerError::from(err));
    }

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();
    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct UpdateDatabaseParams {
    identity: String,
    name: String,
}

async fn update_database(state: &mut State) -> SimpleHandlerResult {
    let UpdateDatabaseParams { identity, name } = UpdateDatabaseParams::take_from(state);
    let body = state.borrow_mut::<Body>();
    let data = hyper::body::to_bytes(body).await;
    let data = match data {
        Ok(data) => data,
        Err(_) => return Err(HandlerError::from(anyhow!("Invalid request body")).with_status(StatusCode::BAD_REQUEST)),
    };
    let wasm_bytes = data.to_vec();
    let wasm_bytes_address = hash_bytes(&wasm_bytes);
    object_db::insert_object(wasm_bytes).await.unwrap();

    let identity = Hash::from_hex(&identity).unwrap();
    let num_replicas = 1;

    if let Err(err) = controller::update_database(&identity, &name, &wasm_bytes_address, num_replicas).await {
        log::debug!("{err}");
        return Err(HandlerError::from(err));
    }

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();
    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct DeleteDatabaseParams {
    identity: String,
    name: String,
}

async fn delete_database(state: &mut State) -> SimpleHandlerResult {
    let DeleteDatabaseParams { identity, name } = DeleteDatabaseParams::take_from(state);
    let identity = Hash::from_hex(&identity).unwrap();

    if let Err(err) = controller::delete_database(&identity, &name).await {
        log::debug!("{err}");
        return Err(HandlerError::from(err));
    }

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();
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
            .to_async_borrowing(update_database);

        route
            .post("/:identity/:name/delete")
            .with_path_extractor::<DeleteDatabaseParams>()
            .to_async_borrowing(delete_database);

    })
}