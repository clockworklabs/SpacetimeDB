use crate::address::Address;
use crate::hash::hash_bytes;
use crate::hash::Hash;
use crate::nodes::control_node::control_db;
use crate::nodes::control_node::controller;
use crate::nodes::control_node::object_db;
use crate::protobuf::control_db::HostType;
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
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DNSResponse {
    address: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct DNSParams {
    database_name: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct DNSQueryParams {}

async fn dns(state: &mut State) -> SimpleHandlerResult {
    let DNSParams { database_name } = DNSParams::take_from(state);
    let DNSQueryParams {} = DNSQueryParams::take_from(state);

    let address = control_db::spacetime_dns(&database_name).await?;
    if let Some(address) = address {
        let response = DNSResponse {
            address: address.to_hex(),
        };

        let json = serde_json::to_string(&response).unwrap();
        let body = Body::from(json);
        let res = Response::builder().status(StatusCode::OK).body(body).unwrap();
        Ok(res)
    } else {
        let res = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap();
        Ok(res)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InitDatabaseResponse {
    address: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct InitDatabaseParams {}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct InitDatabaseQueryParams {
    host_type: Option<String>,
    force: Option<bool>,
    identity: Option<String>,
    name: Option<String>,
}

async fn init_database(state: &mut State) -> SimpleHandlerResult {
    let InitDatabaseParams {} = InitDatabaseParams::take_from(state);
    let InitDatabaseQueryParams {
        identity,
        name,
        host_type,
        force,
    } = InitDatabaseQueryParams::take_from(state);
    let force = force.unwrap_or(false);

    let address = if let Some(name) = name {
        if let Some(address_for_name) = control_db::spacetime_dns(&name).await? {
            if !force {
                Err(anyhow::anyhow!("Pass force true to overwrite database."))?;
            }
            // TODO(cloutiertyler): Validate that the creator has credentials for this database
            address_for_name
        } else {
            // Client specified a name which doesn't yet exist
            // Create a new DNS record and a new address to assign to it
            let new_address = control_db::alloc_spacetime_address().await?;
            control_db::spacetime_insert_dns_record(&new_address, &name).await?;
            new_address
        }
    } else {
        control_db::alloc_spacetime_address().await?
    };

    let identity = if let Some(identity) = identity {
        // TODO(cloutiertyler): Validate that the creator has credentials for this identity
        Hash::from_hex(&identity)?
    } else {
        control_db::alloc_spacetime_identity().await?
    };

    let host_type = match host_type {
        None => HostType::Wasm32,
        Some(ht) => match ht.parse() {
            Ok(ht) => ht,
            Err(_) => {
                return Err(HandlerError::from(anyhow!("unknown host type {ht}")).with_status(StatusCode::BAD_REQUEST))
            }
        },
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
        controller::insert_database(&address, &identity, &program_bytes_addr, host_type, num_replicas, force).await
    {
        log::debug!("{err}");
        return Err(HandlerError::from(err));
    }

    let response = InitDatabaseResponse {
        address: address.to_hex(),
    };
    let json = serde_json::to_string(&response).unwrap();
    let res = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(json))
        .unwrap();
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
            .get("/dns/:database_name")
            .with_path_extractor::<DNSParams>()
            .with_query_string_extractor::<DNSQueryParams>()
            .to_async_borrowing(dns);

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
