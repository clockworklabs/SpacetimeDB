use crate::address::Address;
use crate::hash::hash_bytes;
use crate::hash::Hash;
use crate::nodes::control_node::control_db;
use crate::nodes::control_node::controller;
use crate::nodes::control_node::object_db;
use crate::nodes::HostType;
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
struct InitDatabaseParams {}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct InitDatabaseQueryParams {
    host_type: Option<String>,
    identity: Option<String>,
}

async fn init_database(state: &mut State) -> SimpleHandlerResult {
    let InitDatabaseParams {} = InitDatabaseParams::take_from(state);
    let InitDatabaseQueryParams { identity, host_type } = InitDatabaseQueryParams::take_from(state);

    let identity = if let Some(identity) = identity {
        // TODO(cloutiertyler): Validate that the creator has credentials for this identity
        Hash::from_hex(&identity)?
    } else {
        control_db::alloc_spacetime_identity().await?
    };

    let address = control_db::alloc_spacetime_address().await?;

    let host_type = match HostType::parse(host_type) {
        Ok(ht) => ht,
        Err(e) => return Err(HandlerError::from(e).with_status(StatusCode::BAD_REQUEST)),
    };

    let body = state.borrow_mut::<Body>();
    let data = hyper::body::to_bytes(body).await;
    let data = match data {
        Ok(data) => data,
        Err(_) => return Err(HandlerError::from(anyhow!("Invalid request body")).with_status(StatusCode::BAD_REQUEST)),
    };
    let program_bytes = data.to_vec();
    let program_bytes_addr = hash_bytes(&program_bytes);
    object_db::insert_object(program_bytes).await.unwrap();

    let num_replicas = 1;

    if let Err(err) =
        controller::insert_database(&address, &identity, &program_bytes_addr, host_type, num_replicas, false).await
    {
        log::debug!("{err}");
        return Err(HandlerError::from(err));
    }

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();
    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct UpdateDatabaseParams {
    address: String,
}

async fn update_database(state: &mut State) -> SimpleHandlerResult {
    let UpdateDatabaseParams { address } = UpdateDatabaseParams::take_from(state);

    // TODO(cloutiertyler): Validate that the creator has credentials for the identity of this database

    let address = Address::from_hex(&address)?;

    let body = state.borrow_mut::<Body>();
    let data = hyper::body::to_bytes(body).await;
    let data = match data {
        Ok(data) => data,
        Err(_) => return Err(HandlerError::from(anyhow!("Invalid request body")).with_status(StatusCode::BAD_REQUEST)),
    };
    let program_bytes = data.to_vec();
    let program_bytes_address = hash_bytes(&program_bytes);
    object_db::insert_object(program_bytes).await.unwrap();

    let num_replicas = 1;

    if let Err(err) = controller::update_database(&address, &program_bytes_address, num_replicas).await {
        log::debug!("{err}");
        return Err(HandlerError::from(err));
    }

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();
    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct DeleteDatabaseParams {
    address: String,
}

async fn delete_database(state: &mut State) -> SimpleHandlerResult {
    let DeleteDatabaseParams { address } = DeleteDatabaseParams::take_from(state);

    // TODO(cloutiertyler): Validate that the creator has credentials for the identity of this database

    let address = Address::from_hex(&address)?;

    if let Err(err) = controller::delete_database(&address).await {
        log::debug!("{err}");
        return Err(HandlerError::from(err));
    }

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();
    Ok(res)
}

pub fn router() -> Router {
    build_simple_router(|route| {
        route
            .post("/init")
            .with_path_extractor::<InitDatabaseParams>()
            .with_query_string_extractor::<InitDatabaseQueryParams>()
            .to_async_borrowing(init_database);

        route
            .post("/update/:address")
            .with_path_extractor::<UpdateDatabaseParams>()
            .to_async_borrowing(update_database);

        route
            .post("/delete/:address")
            .with_path_extractor::<DeleteDatabaseParams>()
            .to_async_borrowing(delete_database);
    })
}
