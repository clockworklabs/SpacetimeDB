use std::collections::HashMap;

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
use spacetimedb::host::InvalidReducerArguments;
use spacetimedb_lib::{ElementDef, EntityDef, TypeDef};

use crate::auth::get_or_create_creds_from_header;
use crate::nodes::worker_node::client_api::proxy::proxy_to_control_node_client_api;
use crate::nodes::worker_node::client_api::routes::database::DBCallErr::NoSuchDatabase;
use crate::nodes::worker_node::worker_db;
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

async fn call(state: &mut State) -> SimpleHandlerResult {
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

    let database = match worker_db::get_database_by_address(&address) {
        Some(database) => database,
        None => {
            log::error!("Could not find database: {}", address.to_hex());
            return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND));
        }
    };
    let database_instance = match worker_db::get_leader_database_instance_by_database(database.id) {
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
                    Hash::from_slice(database.identity).to_hex(),
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
async fn extract_db_call_info(state: &mut State, address: &Address) -> Result<DatabaseInformation, DBCallErr> {
    let headers = state.borrow::<HeaderMap>();

    let auth_header = headers.get(AUTHORIZATION);
    // Passing create true because we don't ever want this to fail.
    let creds = get_or_create_creds_from_header(auth_header, true).await?.unwrap();
    let (caller_identity, caller_identity_token) = creds;

    let database = match worker_db::get_database_by_address(address) {
        Some(database) => database,
        None => return Err(DBCallErr::NoSuchDatabase),
    };

    let database_instance = match worker_db::get_leader_database_instance_by_database(database.id) {
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
        NoSuchDatabase => {
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
        EntityDef::Repeater(_) => 2,
    };
    if expand {
        // TODO(noa): make this less hacky; needs coordination w/ spacetime-web
        let schema = match description {
            EntityDef::Table(table) => json!(table.tuple),
            EntityDef::Reducer(r) => json!({
                "name": r.name,
                "elements": r.args,
            }),
            EntityDef::Repeater(r) => json!({
                "name": r.name,
                "elements": [
                    ElementDef {
                        tag: 0,
                        name: Some(String::from("timestamp")),
                        element_type: TypeDef::U64,
                    },
                    ElementDef {
                        tag: 1,
                        name: Some(String::from("delta_time")),
                        element_type: TypeDef::U64,
                    },
                ],
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

async fn describe(state: &mut State) -> SimpleHandlerResult {
    let DescribeParams {
        address,
        entity_type,
        entity,
    } = DescribeParams::take_from(state);

    let DescribeQueryParams { expand } = DescribeQueryParams::take_from(state);

    let address = Address::from_hex(&address)?;

    let call_info = match extract_db_call_info(state, &address).await {
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
async fn catalog(state: &mut State) -> SimpleHandlerResult {
    let CatalogParams { address } = CatalogParams::take_from(state);
    let DescribeQueryParams { expand } = DescribeQueryParams::take_from(state);

    let address = Address::from_hex(&address)?;

    let call_info = match extract_db_call_info(state, &address).await {
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

async fn logs(state: &mut State) -> SimpleHandlerResult {
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

    let database = match worker_db::get_database_by_address(&address) {
        Some(database) => database,
        None => return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND)),
    };

    let database_identity = Hash::from_slice(database.identity);

    if database_identity != caller_identity {
        return Err(HandlerError::from(anyhow!("Identity does not own database.")).with_status(StatusCode::BAD_REQUEST));
    }

    let database_instance = worker_db::get_leader_database_instance_by_database(database.id);
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

async fn sql(state: &mut State) -> SimpleHandlerResult {
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

    let database = match worker_db::get_database_by_address(&address) {
        Some(database) => database,
        None => return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND)),
    };

    let database_identity = Hash::from_slice(database.identity);

    if database_identity != caller_identity {
        return Err(HandlerError::from(anyhow!("Identity does not own database.")).with_status(StatusCode::BAD_REQUEST));
    }

    let database_instance = worker_db::get_leader_database_instance_by_database(database.id);
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

    let results = match sql::execute(instance_id, sql_text) {
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

// TODO(cloutiertyler): all references to address should become name_or_address
pub fn router() -> Router {
    build_simple_router(|route| {
        route
            .get("/dns/:database_name")
            .to_async(proxy_to_control_node_client_api);
        route
            .get("/reverse_dns/:address")
            .to_async(proxy_to_control_node_client_api);
        route.post("/publish").to_async(proxy_to_control_node_client_api);

        route
            .post("/delete/:address")
            .to_async(proxy_to_control_node_client_api);
        route.post("/set_name").to_async(proxy_to_control_node_client_api);
        route
            .get("/subscribe")
            .with_path_extractor::<SubscribeParams>()
            .with_query_string_extractor::<SubscribeQueryParams>()
            .to_async(handle_websocket);

        route
            .post("/call/:address/:reducer")
            .with_path_extractor::<CallParams>()
            .to_async_borrowing(call);

        route
            .get("/schema/:address/:entity_type/:entity")
            .with_path_extractor::<DescribeParams>()
            .with_query_string_extractor::<DescribeQueryParams>()
            .to_async_borrowing(describe);

        route
            .get("/schema/:address")
            .with_path_extractor::<CatalogParams>()
            .with_query_string_extractor::<DescribeQueryParams>()
            .to_async_borrowing(catalog);

        route
            .get("/logs/:address")
            .with_path_extractor::<LogsParams>()
            .with_query_string_extractor::<LogsQuery>()
            .to_async_borrowing(logs);

        route
            .post("/sql/:address")
            .with_path_extractor::<SqlParams>()
            .with_query_string_extractor::<SqlQueryParams>()
            .to_async_borrowing(sql);
    })
}
