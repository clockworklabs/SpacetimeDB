use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use gotham::anyhow::anyhow;
use gotham::handler::HandlerError;
use gotham::handler::SimpleHandlerResult;
use gotham::prelude::FromState;
use gotham::prelude::MapHandlerError;
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

use serde_json::{json, Value};
use spacetimedb::control_db::CONTROL_DB;
use spacetimedb::hash::hash_bytes;
use spacetimedb::host::InvalidReducerArguments;
use spacetimedb::protobuf::control_db::HostType;
use spacetimedb_lib::{name, EntityDef};

use crate::auth::get_or_create_creds_from_header;
use crate::{ApiCtx, Controller, ControllerCtx};
use spacetimedb::address::Address;
use spacetimedb::database_logger::DatabaseLogger;
use spacetimedb::hash::Hash;
use spacetimedb::host::host_controller;
use spacetimedb::host::host_controller::DescribedEntityType;
use spacetimedb::host::ReducerArgs;
use spacetimedb::json::client_api::StmtResultJson;
use spacetimedb::protobuf::control_db::DatabaseInstance;
use spacetimedb::sql;

use super::subscribe::handle_websocket;
use super::subscribe::SubscribeParams;
use super::subscribe::SubscribeQueryParams;

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct CallParams {
    address: String,
    reducer: String,
}

async fn call(ctx: &dyn ApiCtx, state: &mut State) -> SimpleHandlerResult {
    let CallParams { address, reducer } = CallParams::take_from(state);

    let address = Address::from_hex(&address)?;

    let headers = state.borrow::<HeaderMap>();
    let auth_header = headers.get(AUTHORIZATION);

    let (caller_identity, caller_identity_token) = get_or_create_creds_from_header(auth_header, true).await?.unwrap();

    let body = state.borrow_mut::<Body>();
    let data = hyper::body::to_bytes(body).await?;
    if data.is_empty() {
        return Err(HandlerError::from(anyhow!("Missing request body.")).with_status(StatusCode::BAD_REQUEST));
    }
    let args = ReducerArgs::Json(data);

    let database = match ctx.get_database_by_address(&address).await? {
        Some(database) => database,
        None => {
            log::error!("Could not find database: {}", address.to_hex());
            return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND));
        }
    };
    let database_instance = match ctx.get_leader_database_instance_by_database(database.id).await {
        Some(database) => database,
        None => {
            return Err(
                HandlerError::from(anyhow!("Database instance not scheduled to this node yet."))
                    .with_status(StatusCode::NOT_FOUND),
            )
        }
    };
    let instance_id = database_instance.id;
    let host = host_controller::get_host();

    let result = match host.call_reducer(instance_id, caller_identity, &reducer, args).await {
        Ok(Some(rcr)) => {
            if rcr.budget_exceeded {
                log::warn!(
                    "Node's energy budget exceeded for identity: {} while executing {}",
                    Hash::from_slice(&database.identity),
                    reducer
                );
                return Err(HandlerError::from(anyhow!("Module energy budget exhausted."))
                    .with_status(StatusCode::PAYMENT_REQUIRED));
            }
            rcr
        }
        Ok(None) => {
            log::debug!("Attempt to call non-existent reducer {}", reducer);
            return Err(HandlerError::from(anyhow!("reducer not found")).with_status(StatusCode::NOT_FOUND));
        }
        Err(e) => {
            let status_code = if e.is::<InvalidReducerArguments>() {
                log::debug!("Attempt to call reducer with invalid arguments");
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            };

            log::debug!("Error while invoking reducer {}", e);
            return Err(HandlerError::from(e).with_status(status_code));
        }
    };

    let res = Response::builder()
        .header("Spacetime-Identity", caller_identity.to_hex())
        .header("Spacetime-Identity-Token", caller_identity_token)
        .header("Spacetime-Energy-Used", result.energy_quanta_used)
        .header(
            "Spacetime-Execution-Duration-Micros",
            result.host_execution_duration.as_micros().to_string(),
        )
        .status(StatusCode::OK)
        .body(Body::empty())
        .unwrap();
    Ok(res)
}

#[derive(Debug)]
enum DBCallErr {
    HandlerError(HandlerError),
    NoSuchDatabase,
    InstanceNotScheduled,
}

use spacetimedb_lib::name::{DnsLookupResponse, InsertDomainResult, PublishResult};
use std::convert::From;

impl From<HandlerError> for DBCallErr {
    fn from(error: HandlerError) -> Self {
        DBCallErr::HandlerError(error)
    }
}

struct DatabaseInformation {
    database_instance: DatabaseInstance,
    caller_identity: Hash,
    caller_identity_token: String,
}
/// Extract some common parameters that most API call invocations to the database will use.
/// TODO(tyler): Ryan originally intended for extract call info to be used for any call that is specific to a
/// database. However, there are some functions that should be callable from anyone, possibly even if they
/// don't provide any credentials at all. The problem is that this function doesn't make sense in all places
/// where credentials are required (e.g. publish), so for now we're just going to keep this as is, but we're
/// going to generate a new set of credentials if you don't provide them.
async fn extract_db_call_info(
    ctx: &dyn ApiCtx,
    state: &mut State,
    address: &Address,
) -> Result<DatabaseInformation, DBCallErr> {
    let headers = state.borrow::<HeaderMap>();

    let auth_header = headers.get(AUTHORIZATION);
    // Passing create true because we don't ever want this to fail.
    let creds = get_or_create_creds_from_header(auth_header, true).await?.unwrap();
    let (caller_identity, caller_identity_token) = creds;

    let database = match ctx.get_database_by_address(address).await.unwrap() {
        Some(database) => database,
        None => return Err(DBCallErr::NoSuchDatabase),
    };

    let database_instance = match ctx.get_leader_database_instance_by_database(database.id).await {
        Some(database) => database,
        None => {
            return Err(DBCallErr::InstanceNotScheduled);
        }
    };
    Ok(DatabaseInformation {
        database_instance,
        caller_identity,
        caller_identity_token,
    })
}

fn handle_db_err(address: &Address, err: DBCallErr) -> SimpleHandlerResult {
    match err {
        DBCallErr::NoSuchDatabase => {
            log::error!("Could not find database: {}", address.to_hex());
            Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND))
        }
        DBCallErr::InstanceNotScheduled => Err(HandlerError::from(anyhow!(
            "Database instance not scheduled to this node yet."
        ))
        .with_status(StatusCode::NOT_FOUND)),
        DBCallErr::HandlerError(err) => Err(err),
    }
}

fn entity_description_json(description: &EntityDef, expand: bool) -> Value {
    let typ = DescribedEntityType::from_entitydef(description).as_str();
    let len = match description {
        EntityDef::Table(t) => t.tuple.elements.len(),
        EntityDef::Reducer(r) => r.args.len(),
    };
    if expand {
        // TODO(noa): make this less hacky; needs coordination w/ spacetime-web
        let schema = match description {
            EntityDef::Table(table) => json!(table.tuple),
            EntityDef::Reducer(r) => json!({
                "name": r.name,
                "elements": r.args,
            }),
        };
        json!({
            "type": typ,
            "arity": len,
            "schema": schema
        })
    } else {
        json!({
            "type": typ,
            "arity": len,
        })
    }
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct DescribeParams {
    address: String,
    entity_type: String,
    entity: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct DescribeQueryParams {
    expand: Option<bool>,
}

async fn describe(ctx: &dyn ApiCtx, state: &mut State) -> SimpleHandlerResult {
    let DescribeParams {
        address,
        entity_type,
        entity,
    } = DescribeParams::take_from(state);

    let DescribeQueryParams { expand } = DescribeQueryParams::take_from(state);

    let address = Address::from_hex(&address)?;

    let call_info = match extract_db_call_info(ctx, state, &address).await {
        Ok(p) => p,
        Err(e) => return handle_db_err(&address, e),
    };

    let instance_id = call_info.database_instance.id;
    let host = host_controller::get_host();

    let entity_type = entity_type.as_str().parse().map_err(|()| {
        log::debug!("Request to describe unhandled entity type: {}", entity_type);
        HandlerError::from(anyhow!("Invalid entity type for description: {}", entity_type))
            .with_status(StatusCode::NOT_FOUND)
    })?;
    let catalog = host.catalog(instance_id).map_err_with_status(StatusCode::NOT_FOUND)?;
    let description = catalog
        .get(&entity)
        .filter(|desc| DescribedEntityType::from_entitydef(desc) == entity_type)
        .with_context(|| format!("{entity_type} {entity:?} not found"))
        .map_err_with_status(StatusCode::NOT_FOUND)?;

    let expand = expand.unwrap_or(true);
    let response_json = json!({ entity: entity_description_json(description, expand) });

    let response = Response::builder()
        .header("Spacetime-Identity", call_info.caller_identity.to_hex())
        .header("Spacetime-Identity-Token", call_info.caller_identity_token)
        .status(StatusCode::OK)
        .body(Body::from(response_json.to_string()))
        .unwrap();

    Ok(response)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct CatalogParams {
    address: String,
}
async fn catalog(ctx: &dyn ApiCtx, state: &mut State) -> SimpleHandlerResult {
    let CatalogParams { address } = CatalogParams::take_from(state);
    let DescribeQueryParams { expand } = DescribeQueryParams::take_from(state);

    let address = Address::from_hex(&address)?;

    let call_info = match extract_db_call_info(ctx, state, &address).await {
        Ok(p) => p,
        Err(e) => return handle_db_err(&address, e),
    };

    let instance_id = call_info.database_instance.id;
    let host = host_controller::get_host();
    let catalog = host.catalog(instance_id).map_err_with_status(StatusCode::NOT_FOUND)?;
    let expand = expand.unwrap_or(false);
    let response_catalog: HashMap<_, _> = catalog
        .iter()
        .map(|(name, entity)| (name, entity_description_json(entity, expand)))
        .collect();
    let response_json = json!(response_catalog);

    let response = Response::builder()
        .header("Spacetime-Identity", call_info.caller_identity.to_hex())
        .header("Spacetime-Identity-Token", call_info.caller_identity_token)
        .status(StatusCode::OK)
        .body(Body::from(response_json.to_string()))
        .unwrap();

    Ok(response)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct LogsParams {
    address: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct LogsQuery {
    num_lines: Option<u32>,
}

async fn logs(ctx: &dyn ApiCtx, state: &mut State) -> SimpleHandlerResult {
    let LogsParams { address } = LogsParams::take_from(state);
    let LogsQuery { num_lines } = LogsQuery::take_from(state);

    let headers = state.borrow::<HeaderMap>();
    let auth_header = headers.get(AUTHORIZATION);

    // You should not be able to read the logs from a database that you do not own
    // so, unless you are the owner, this will fail, hence `create = false`.
    let creds = get_or_create_creds_from_header(auth_header, false).await?;
    if let None = creds {
        return Err(HandlerError::from(anyhow!("Invalid credentials.")).with_status(StatusCode::BAD_REQUEST));
    }
    let (caller_identity, _) = creds.unwrap();

    let address = Address::from_hex(&address)?;

    let database = match ctx.get_database_by_address(&address).await? {
        Some(database) => database,
        None => return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND)),
    };

    let database_identity = Hash::from_slice(&database.identity);

    if database_identity != caller_identity {
        return Err(HandlerError::from(anyhow!(
            "Identity does not own database, expected: {} got: {}",
            database_identity.to_hex(),
            caller_identity.to_hex()
        ))
        .with_status(StatusCode::BAD_REQUEST));
    }

    let database_instance = ctx.get_leader_database_instance_by_database(database.id).await;
    let instance_id = database_instance.unwrap().id;

    let filepath = DatabaseLogger::filepath(&address, instance_id);
    let lines = DatabaseLogger::read_latest(&filepath, num_lines).await;

    let res = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(lines))
        .unwrap();

    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct SqlParams {
    address: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct SqlQueryParams {}

async fn sql(ctx: &dyn ApiCtx, state: &mut State) -> SimpleHandlerResult {
    let SqlParams { address } = SqlParams::take_from(state);
    let SqlQueryParams {} = SqlQueryParams::take_from(state);

    let headers = state.borrow::<HeaderMap>();
    let auth_header = headers.get(AUTHORIZATION);

    // You should not be able to query a database that you do not own
    // so, unless you are the owner, this will fail, hence `create = false`.
    let creds = get_or_create_creds_from_header(auth_header, false).await?;
    if let None = creds {
        return Err(HandlerError::from(anyhow!("Invalid credentials.")).with_status(StatusCode::BAD_REQUEST));
    }
    let (caller_identity, _) = creds.unwrap();

    let address = Address::from_hex(&address)?;

    let database = match ctx.get_database_by_address(&address).await.unwrap() {
        Some(database) => database,
        None => return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND)),
    };

    let database_identity = Hash::from_slice(&database.identity);

    if database_identity != caller_identity {
        return Err(HandlerError::from(anyhow!("Identity does not own database.")).with_status(StatusCode::BAD_REQUEST));
    }

    let database_instance = ctx.get_leader_database_instance_by_database(database.id).await;
    let instance_id = database_instance.unwrap().id;

    let body = state.borrow_mut::<Body>();
    let data = hyper::body::to_bytes(body).await?;
    if data.len() == 0 {
        return Err(HandlerError::from(anyhow!("Missing request body.")).with_status(StatusCode::BAD_REQUEST));
    }

    let sql_text = match String::from_utf8(data.to_vec()) {
        Ok(s) => s,
        Err(err) => {
            log::debug!("{:?}", err);
            return Err(HandlerError::from(anyhow!("Invalid query string.")).with_status(StatusCode::BAD_REQUEST));
        }
    };

    let results = match sql::execute(ctx.database_instance_context_controller(), instance_id, sql_text) {
        Ok(results) => results,
        Err(err) => {
            log::warn!("{}", err);
            let res = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap();
            return Ok(res);
        }
    };
    let mut json = Vec::new();

    for result in results {
        let stmt_result = match result {
            Ok(result) => result,
            Err(err) => {
                log::warn!("{}", err);
                continue;
            }
        };
        let stmt_res_json = StmtResultJson {
            schema: stmt_result.schema,
            rows: stmt_result.rows.iter().map(|x| x.elements.to_vec()).collect::<Vec<_>>(),
        };
        json.push(stmt_res_json)
    }
    let body = serde_json::to_string_pretty(&json).unwrap();
    let res = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(body))
        .unwrap();

    Ok(res)
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

async fn dns(_ctx: &dyn ControllerCtx, state: &mut State) -> SimpleHandlerResult {
    let DNSParams { database_name } = DNSParams::take_from(state);
    let DNSQueryParams {} = DNSQueryParams::take_from(state);

    let address = CONTROL_DB.spacetime_dns(&database_name).await?;
    let response = if let Some(address) = address {
        DnsLookupResponse::Success {
            domain: database_name,
            address: address.to_hex(),
        }
    } else {
        DnsLookupResponse::Failure { domain: database_name }
    };

    let json = serde_json::to_string(&response).unwrap();
    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(json))
        .unwrap())
}

async fn reverse_dns(_ctx: &dyn ControllerCtx, state: &mut State) -> SimpleHandlerResult {
    let ReverseDNSParams { database_address } = ReverseDNSParams::take_from(state);
    let address = Address::from_hex(&database_address)?;
    let names = CONTROL_DB.spacetime_reverse_dns(&address).await?;

    let response = name::ReverseDNSResponse { names };
    let json = serde_json::to_string(&response).unwrap();
    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(json))
        .unwrap())
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct RegisterTldParams {
    tld: String,
}

async fn register_tld(_ctx: &dyn ControllerCtx, state: &mut State) -> SimpleHandlerResult {
    let RegisterTldParams { tld } = RegisterTldParams::take_from(state);
    let headers = state.borrow::<HeaderMap>();
    let auth_header = headers.get(AUTHORIZATION);

    // You should not be able to publish to a database that you do not own
    // so, unless you are the owner, this will fail, hence `create = false`.
    let creds = get_or_create_creds_from_header(auth_header, false).await?;
    if let None = creds {
        return Err(HandlerError::from(anyhow!("Invalid credentials.")).with_status(StatusCode::BAD_REQUEST));
    }
    let (caller_identity, _) = creds.unwrap();

    let result = CONTROL_DB.spacetime_register_tld(tld.as_str(), caller_identity).await?;

    let json = serde_json::to_string(&result).unwrap();
    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(json))
        .unwrap())
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct PublishDatabaseParams {}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct PublishDatabaseQueryParams {
    host_type: Option<String>,
    clear: Option<bool>,
    name_or_address: Option<String>,
    trace_log: Option<bool>,
    register_tld: Option<bool>,
}

#[cfg(not(feature = "tracelogging"))]
fn should_trace(_trace_log: Option<bool>) -> bool {
    false
}

#[cfg(feature = "tracelogging")]
fn should_trace(trace_log: Option<bool>) -> bool {
    trace_log.unwrap_or(false)
}

async fn publish(ctx: &dyn ControllerCtx, state: &mut State) -> SimpleHandlerResult {
    let PublishDatabaseParams {} = PublishDatabaseParams::take_from(state);
    let PublishDatabaseQueryParams {
        name_or_address,
        host_type,
        clear,
        trace_log,
        register_tld,
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
    let (caller_identity, _) = creds.unwrap();

    // Parse the address or convert the name to a usable address
    let (db_address, specified_address) = if let Some(name_or_address) = name_or_address.clone() {
        if let Ok(address) = Address::from_hex(&name_or_address) {
            // All addresses are invalid names
            (address, true)
        } else {
            // If it's not a valid address it must be a name
            if let Some(address) = CONTROL_DB.spacetime_dns(&name_or_address).await? {
                // TODO(cloutiertyler): Validate that the creator has credentials for this database
                (address, false)
            } else {
                // Client specified a name which doesn't yet exist
                // Create a new DNS record and a new address to assign to it
                let address = CONTROL_DB.alloc_spacetime_address().await?;
                let result = CONTROL_DB
                    .spacetime_insert_domain(
                        &address,
                        &name_or_address,
                        caller_identity,
                        register_tld.unwrap_or_default(),
                    )
                    .await?;
                match result {
                    InsertDomainResult::Success { .. } => {}
                    InsertDomainResult::TldNotRegistered { domain } => {
                        let json = serde_json::to_string(&PublishResult::TldNotRegistered { domain })?;
                        let res = Response::builder()
                            .status(StatusCode::OK)
                            .body(Body::from(json))
                            .unwrap();
                        return Ok(res);
                    }
                    InsertDomainResult::PermissionDenied { domain } => {
                        let json = serde_json::to_string(&PublishResult::PermissionDenied { domain })?;
                        let res = Response::builder()
                            .status(StatusCode::OK)
                            .body(Body::from(json))
                            .unwrap();
                        return Ok(res);
                    }
                }

                (address, false)
            }
        }
    } else {
        // No domain or address was specified, create a new one
        (CONTROL_DB.alloc_spacetime_address().await?, false)
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
    ctx.object_db().insert_object(program_bytes).unwrap();

    let num_replicas = 1;

    let trace_log = should_trace(trace_log);

    match ctx.get_database_by_address(&db_address).await {
        Ok(database) => match database {
            Some(db) => {
                if Hash::from_slice(db.identity.as_slice()) != caller_identity {
                    return Err(HandlerError::from(anyhow!("Identity does not own this database."))
                        .with_status(StatusCode::BAD_REQUEST));
                }

                if clear {
                    if let Err(err) = Controller::insert_database(
                        ctx,
                        &db_address,
                        &caller_identity,
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
                    Controller::update_database(ctx, &db_address, &program_bytes_addr, num_replicas).await
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

                if let Err(err) = Controller::insert_database(
                    ctx,
                    &db_address,
                    &caller_identity,
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

    let response = PublishResult::Success {
        domain: name_or_address,
        address: db_address.to_hex(),
    };
    let json = serde_json::to_string(&response).unwrap();

    //TODO(tyler): Eventually we want it to be possible to publish a database
    // which no one has the credentials to. In that case we wouldn't want to
    // return a token.
    let res = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(json))
        .unwrap();
    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct DeleteDatabaseParams {
    address: String,
}

async fn delete_database(ctx: &dyn ControllerCtx, state: &mut State) -> SimpleHandlerResult {
    let DeleteDatabaseParams { address } = DeleteDatabaseParams::take_from(state);

    // TODO(cloutiertyler): Validate that the creator has credentials for the identity of this database

    let address = Address::from_hex(&address)?;

    if let Err(err) = Controller::delete_database(ctx, &address).await {
        log::debug!("{err}");
        return Err(HandlerError::from(err));
    }

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();
    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct SetNameQueryParams {
    domain: String,
    address: String,
    register_tld: Option<bool>,
}

async fn set_name(ctx: &dyn ControllerCtx, state: &mut State) -> SimpleHandlerResult {
    let SetNameQueryParams {
        address,
        domain,
        register_tld,
    } = SetNameQueryParams::take_from(state);

    let headers = state.borrow::<HeaderMap>();
    let auth_header = headers.get(AUTHORIZATION);

    let creds = get_or_create_creds_from_header(auth_header, false).await?;
    if let None = creds {
        return Err(HandlerError::from(anyhow!("Invalid credentials.")).with_status(StatusCode::BAD_REQUEST));
    }
    let (caller_identity, _) = creds.unwrap();
    let address = Address::from_hex(&address)?;

    let database = match ctx.get_database_by_address(&address).await? {
        Some(database) => database,
        None => return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND)),
    };

    let database_identity = Hash::from_slice(&database.identity);
    if database_identity != caller_identity {
        return Err(HandlerError::from(anyhow!("Identity does not own database.")).with_status(StatusCode::BAD_REQUEST));
    }

    let response = CONTROL_DB
        .spacetime_insert_domain(&address, &domain, caller_identity, register_tld.unwrap_or_default())
        .await?;

    let json = serde_json::to_string(&response)?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(json))
        .unwrap())
}

pub fn control_router(route: &mut gotham::router::builder::RouterBuilder<'_, (), ()>, ctx: &Arc<dyn ControllerCtx>) {
    {
        route
            .get("/dns/:database_name")
            .with_path_extractor::<DNSParams>()
            .with_query_string_extractor::<DNSQueryParams>()
            .to_new_handler(with_ctx!(ctx, dns));
        route
            .get("/reverse_dns/:database_address")
            .with_path_extractor::<ReverseDNSParams>()
            .to_new_handler(with_ctx!(ctx, reverse_dns));
        route
            .get("/set_name")
            .with_query_string_extractor::<SetNameQueryParams>()
            .to_new_handler(with_ctx!(ctx, set_name));
        route
            .get("/register_tld")
            .with_query_string_extractor::<RegisterTldParams>()
            .to_new_handler(with_ctx!(ctx, register_tld));

        route
            .post("/publish")
            .with_path_extractor::<PublishDatabaseParams>()
            .with_query_string_extractor::<PublishDatabaseQueryParams>()
            .to_new_handler(with_ctx!(ctx, publish));

        route
            .post("/delete/:address")
            .with_path_extractor::<DeleteDatabaseParams>()
            .to_new_handler(with_ctx!(ctx, delete_database));
    }
}

// TODO(cloutiertyler): all references to address should become name_or_address
pub fn router(ctx: &Arc<dyn ApiCtx>, control_ctx: Option<&Arc<dyn ControllerCtx>>) -> Router {
    build_simple_router(|route| {
        if let Some(control_ctx) = control_ctx {
            control_router(route, control_ctx);
        }

        route
            .get("/subscribe")
            .with_path_extractor::<SubscribeParams>()
            .with_query_string_extractor::<SubscribeQueryParams>()
            .to_async_borrowing(handle_websocket);

        route
            .post("/call/:address/:reducer")
            .with_path_extractor::<CallParams>()
            .to_new_handler(with_ctx!(ctx, call));

        route
            .get("/schema/:address/:entity_type/:entity")
            .with_path_extractor::<DescribeParams>()
            .with_query_string_extractor::<DescribeQueryParams>()
            .to_new_handler(with_ctx!(ctx, describe));

        route
            .get("/schema/:address")
            .with_path_extractor::<CatalogParams>()
            .with_query_string_extractor::<DescribeQueryParams>()
            .to_new_handler(with_ctx!(ctx, catalog));

        route
            .get("/logs/:address")
            .with_path_extractor::<LogsParams>()
            .with_query_string_extractor::<LogsQuery>()
            .to_new_handler(with_ctx!(ctx, logs));

        route
            .post("/sql/:address")
            .with_path_extractor::<SqlParams>()
            .with_query_string_extractor::<SqlQueryParams>()
            .to_new_handler(with_ctx!(ctx, sql));
    })
}
