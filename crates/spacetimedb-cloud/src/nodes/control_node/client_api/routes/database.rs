use crate::auth::get_or_create_creds_from_header;
use crate::nodes::control_node::controller;
use gotham::anyhow::anyhow;
use gotham::handler::HandlerError;
use gotham::handler::SimpleHandlerResult;
use gotham::prelude::FromState;
use gotham::prelude::StaticResponseExtender;
use gotham::router::builder::*;
use gotham::router::Router;
use gotham::state::State;
use gotham::state::StateData;
use hyper::header::AUTHORIZATION;
use hyper::Body;
use hyper::HeaderMap;
use hyper::{Response, StatusCode};
use serde::Deserialize;
use serde::Serialize;
use spacetimedb::address::Address;
use spacetimedb::control_db;
use spacetimedb::hash::hash_bytes;
use spacetimedb::hash::Hash;
use spacetimedb::object_db;
use spacetimedb::protobuf::control_db::HostType;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DNSResponse {
    address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReverseDNSResponse {
    name: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct DNSParams {
    database_name: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct ReverseDNSParams {
    database_address: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct DNSQueryParams {}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct SetNameQueryParams {
    name: String,
    address: String,
}

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

async fn reverse_dns(state: &mut State) -> SimpleHandlerResult {
    let ReverseDNSParams { database_address } = ReverseDNSParams::take_from(state);

    let addr = Address::from_hex(&database_address);

    let name = control_db::spacetime_reverse_dns(&addr.unwrap()).await?;

    if let Some(name) = name {
        let response = ReverseDNSResponse { name: name };
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
struct PublishDatabaseResponse {
    address: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct PublishDatabaseParams {}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct PublishDatabaseQueryParams {
    host_type: Option<String>,
    clear: Option<bool>,
    identity: Option<String>,
    name_or_address: Option<String>,
    trace_log: Option<bool>,
}

#[cfg(not(feature = "tracelogging"))]
fn should_trace(_trace_log: Option<bool>) -> bool {
    false
}

#[cfg(feature = "tracelogging")]
fn should_trace(trace_log: Option<bool>) -> bool {
    trace_log.unwrap_or(false)
}

async fn publish(state: &mut State) -> SimpleHandlerResult {
    let PublishDatabaseParams {} = PublishDatabaseParams::take_from(state);
    let PublishDatabaseQueryParams {
        identity,
        name_or_address,
        host_type,
        clear,
        trace_log,
    } = PublishDatabaseQueryParams::take_from(state);
    let clear = clear.unwrap_or(false);

    let headers = state.borrow::<HeaderMap>();
    let auth_header = headers.get(AUTHORIZATION);

    // You should not be able to publish to a database that you do not own
    // so, unless you are the owner, this will fail, hence `create = false`.
    let creds = get_or_create_creds_from_header(auth_header, true).await?;
    if let None = creds {
        return Err(HandlerError::from(anyhow!("Invalid credentials.")).with_status(StatusCode::BAD_REQUEST));
    }
    let (caller_identity, caller_identity_token) = creds.unwrap();

    // Parse the address or convert the name to a usable address
    let (db_address, specified_address) = if let Some(name_or_address) = name_or_address {
        if let Ok(address) = Address::from_hex(&name_or_address) {
            // All addresses are invalid names
            (address, true)
        } else {
            // If it's not a valid address it must be a name
            if let Some(address) = control_db::spacetime_dns(&name_or_address).await? {
                // TODO(cloutiertyler): Validate that the creator has credentials for this database
                (address, false)
            } else {
                // Client specified a name which doesn't yet exist
                // Create a new DNS record and a new address to assign to it
                let address = control_db::alloc_spacetime_address().await?;
                control_db::spacetime_insert_dns_record(&address, &name_or_address).await?;
                (address, false)
            }
        }
    } else {
        // No name or address was specified, create a new one
        (control_db::alloc_spacetime_address().await?, false)
    };

    let identity = if let Some(identity) = identity {
        // TODO(cloutiertyler): Validate that the creator has credentials for this identity
        Hash::from_hex(&identity)?
    } else {
        control_db::alloc_spacetime_identity().await?
    };

    let host_type = match host_type {
        None => HostType::Wasmer,
        Some(ht) => match ht.parse() {
            Ok(ht) => ht,
            Err(_) => {
                return Err(HandlerError::from(anyhow!("unknown host type {ht}")).with_status(StatusCode::BAD_REQUEST));
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

    let trace_log = should_trace(trace_log);

    match control_db::get_database_by_address(&db_address).await {
        Ok(database) => match database {
            Some(db) => {
                if Hash::from_slice(db.identity.as_slice()) != caller_identity {
                    return Err(HandlerError::from(anyhow!("Identity does not own this database."))
                        .with_status(StatusCode::BAD_REQUEST));
                }

                if clear {
                    if let Err(err) = controller::insert_database(
                        &db_address,
                        &identity,
                        &program_bytes_addr,
                        host_type,
                        num_replicas,
                        clear,
                        trace_log,
                    )
                    .await
                    {
                        log::debug!("{err}");
                        return Err(HandlerError::from(err));
                    }
                } else if let Err(err) =
                    controller::update_database(&db_address, &program_bytes_addr, num_replicas).await
                {
                    log::debug!("{err}");
                    return Err(HandlerError::from(err));
                }
            }
            None => {
                if specified_address {
                    return Err(HandlerError::from(anyhow::anyhow!(
                        "Failed to find database at address: {}",
                        db_address.to_hex()
                    )));
                }

                if let Err(err) = controller::insert_database(
                    &db_address,
                    &identity,
                    &program_bytes_addr,
                    host_type,
                    num_replicas,
                    false,
                    trace_log,
                )
                .await
                {
                    log::debug!("{err}");
                    return Err(HandlerError::from(err));
                }
            }
        },
        Err(e) => {
            return Err(HandlerError::from(e));
        }
    }

    let response = PublishDatabaseResponse {
        address: db_address.to_hex(),
    };
    let json = serde_json::to_string(&response).unwrap();

    // TODO(tyler): Eventually we want it to be possible to publish a database
    // which no one has the credentials to. In that case we wouldn't want to
    // return a token.
    let res = Response::builder()
        .header("Spacetime-Identity", caller_identity.to_hex())
        .header("Spacetime-Identity-Token", caller_identity_token)
        .status(StatusCode::OK)
        .body(Body::from(json))
        .unwrap();
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

async fn set_name(state: &mut State) -> SimpleHandlerResult {
    let SetNameQueryParams { address, name } = SetNameQueryParams::take_from(state);

    let headers = state.borrow::<HeaderMap>();
    let auth_header = headers.get(AUTHORIZATION);

    let creds = get_or_create_creds_from_header(auth_header, false).await?;

    if let None = creds {
        return Err(HandlerError::from(anyhow!("Invalid credentials.")).with_status(StatusCode::BAD_REQUEST));
    }
    let (caller_identity, _) = creds.unwrap();

    let address = Address::from_hex(&address)?;

    let database = match control_db::get_database_by_address(&address).await {
        Ok(database) => database.unwrap(),
        Err(_) => return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND)),
    };

    let database_identity = Hash::from_slice(database.identity);

    if database_identity != caller_identity {
        return Err(HandlerError::from(anyhow!("Identity does not own database.")).with_status(StatusCode::BAD_REQUEST));
    }

    control_db::spacetime_insert_dns_record(&address, &name).await?;

    Ok(Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap())
}

pub fn router() -> Router {
    build_simple_router(|route| {
        route
            .get("/dns/:database_name")
            .with_path_extractor::<DNSParams>()
            .with_query_string_extractor::<DNSQueryParams>()
            .to_async_borrowing(dns);

        route
            .post("/set_name")
            .with_query_string_extractor::<SetNameQueryParams>()
            .to_async_borrowing(set_name);
        route
            .get("/reverse_dns/:database_address")
            .with_path_extractor::<ReverseDNSParams>()
            .to_async_borrowing(reverse_dns);

        route
            .post("/publish")
            .with_path_extractor::<PublishDatabaseParams>()
            .with_query_string_extractor::<PublishDatabaseQueryParams>()
            .to_async_borrowing(publish);
        route
            .post("/delete/:address")
            .with_path_extractor::<DeleteDatabaseParams>()
            .to_async_borrowing(delete_database);
    })
}
