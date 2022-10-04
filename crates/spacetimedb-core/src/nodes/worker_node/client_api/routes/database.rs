use std::collections::HashMap;

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
use serde_json::{json, Value};

use crate::address::Address;
use crate::auth::get_creds_from_header;
use crate::auth::invalid_token_res;
use crate::hash::Hash;
use crate::json::client_api::StmtResultJson;
use crate::nodes::worker_node::client_api::proxy::proxy_to_control_node_client_api;
use crate::nodes::worker_node::client_api::routes::database::DBCallErr::NoSuchDatabase;
use crate::nodes::worker_node::control_node_connection::ControlNodeClient;
use crate::nodes::worker_node::database_logger::DatabaseLogger;
use crate::nodes::worker_node::host::host_controller;
use crate::nodes::worker_node::host::host_controller::{DescribedEntityType, Entity, EntityDescription};
use crate::nodes::worker_node::worker_db;
use crate::protobuf::control_db::DatabaseInstance;
use crate::sql;

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

    for database in worker_db::_get_databases() {
        let address = Address::from_slice(database.address.as_slice());
        log::debug!("Have database {}", address.to_hex());
    }

    for instance in worker_db::get_database_instances() {
        log::debug!("Have instance {:?}", instance);
    }

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

    let result = match host
        .call_reducer(instance_id, caller_identity, &reducer, arg_bytes)
        .await
    {
        Ok(rcr) => {
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
        Err(e) => {
            log::debug!("Unable to call {}", e);
            return Err(HandlerError::from(anyhow!("Database instance not ready."))
                .with_status(StatusCode::SERVICE_UNAVAILABLE));
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

enum DBCallErr {
    InvalidToken,
    NoSuchDatabase,
    InstanceNotScheduled,
}
struct DatabaseInformation {
    database_instance: DatabaseInstance,
    caller_identity: Hash,
    caller_identity_token: String,
}
/// Extract some common parameters that most API call invocations to the database will use.
// TODO(ryan): Use this for call, logs, etc. as well.
async fn extract_db_call_info(state: &mut State, address: &Address) -> Result<DatabaseInformation, DBCallErr> {
    let headers = state.borrow::<HeaderMap>();
    let auth_header = headers.get(AUTHORIZATION);
    let (caller_identity, caller_identity_token) = if let Some(auth_header) = auth_header {
        // Validate the credentials of this connection
        match get_creds_from_header(auth_header) {
            Ok(v) => v,
            Err(_) => return Err(DBCallErr::InvalidToken),
        }
    } else {
        // Generate a new identity if this request doesn't have one already
        let (identity, identity_token) = ControlNodeClient::get_shared().get_new_identity().await.unwrap();
        (identity, identity_token)
    };

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
        DBCallErr::InvalidToken => Ok(invalid_token_res()),
        NoSuchDatabase => {
            log::error!("Could not find database: {}", address.to_hex());
            Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND))
        }
        DBCallErr::InstanceNotScheduled => Err(HandlerError::from(anyhow!(
            "Database instance not scheduled to this node yet."
        ))
        .with_status(StatusCode::NOT_FOUND)),
    }
}

fn entity_description_json(description: &EntityDescription, expand: bool) -> Value {
    if expand {
        json!({
            "type": description.entity.entity_type.as_str(),
            "arity": description.schema.elements.len(),
            "schema": description.schema
        })
    } else {
        json!({
            "type": description.entity.entity_type.as_str(),
            "arity": description.schema.elements.len(),
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

    let entity_type = match entity_type.as_str() {
        "tables" => DescribedEntityType::Table,
        "reducers" => DescribedEntityType::Reducer,
        "repeaters" => DescribedEntityType::RepeatingReducer,
        _ => {
            log::debug!("Request to describe unhandled entity type: {}", entity_type);
            return Err(
                HandlerError::from(anyhow!("Invalid entity type for description: {}", entity_type))
                    .with_status(StatusCode::NOT_FOUND),
            );
        }
    };
    let description = match host
        .describe(
            instance_id,
            Entity {
                entity_name: entity.clone(),
                entity_type: entity_type.clone(),
            },
        )
        .await
    {
        Ok(Some(description)) => description,
        Ok(None) => {
            return Err(
                HandlerError::from(anyhow!("{} not found {}", entity_type.as_str(), entity.clone()))
                    .with_status(StatusCode::NOT_FOUND),
            )
        }
        Err(e) => {
            log::error!("{}", e);
            return Err(HandlerError::from(anyhow!("Database instance not ready."))
                .with_status(StatusCode::SERVICE_UNAVAILABLE));
        }
    };

    let expand = expand.unwrap_or(true);
    let response_json =
        json!({ description.entity.entity_name.clone(): entity_description_json(&description, expand) });

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
    let catalog = match host.catalog(instance_id).await {
        Ok(catalog) => catalog,
        Err(e) => {
            log::error!("{}", e);
            return Err(HandlerError::from(anyhow!("Database instance not ready."))
                .with_status(StatusCode::SERVICE_UNAVAILABLE));
        }
    };
    let expand = expand.unwrap_or(false);
    let response_catalog: HashMap<_, _> = catalog
        .iter()
        .map(|ed| (ed.entity.entity_name.clone(), entity_description_json(&ed, expand)))
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
    num_lines: u32,
}

async fn logs(state: &mut State) -> SimpleHandlerResult {
    let LogsParams { address } = LogsParams::take_from(state);
    let LogsQuery { num_lines } = LogsQuery::take_from(state);

    // TODO(cloutiertyler): Validate that the creator has credentials for the identity of this database

    let address = Address::from_hex(&address)?;

    let database = match worker_db::get_database_by_address(&address) {
        Some(database) => database,
        None => return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND)),
    };
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

    let address = Address::from_hex(&address)?;

    let database = match worker_db::get_database_by_address(&address) {
        Some(database) => database,
        None => return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND)),
    };
    let database_instance = worker_db::get_leader_database_instance_by_database(database.id);
    let instance_id = database_instance.unwrap().id;

    let body = state.borrow_mut::<Body>();
    let data = body.data().await;
    if data.is_none() {
        return Err(HandlerError::from(anyhow!("Missing request body.")).with_status(StatusCode::BAD_REQUEST));
    }
    let data = data.unwrap();

    let sql_text = match String::from_utf8(data.unwrap().to_vec()) {
        Ok(s) => s,
        Err(err) => {
            log::debug!("{:?}", err);
            return Err(HandlerError::from(anyhow!("Invalid query string.")).with_status(StatusCode::BAD_REQUEST));
        }
    };

    let results = sql::execute(instance_id, sql_text);
    let mut json = Vec::new();

    for result in results {
        let stmt_result = result.unwrap();
        let stmt_res_json = StmtResultJson {
            schema: stmt_result.schema,
            rows: stmt_result.rows.iter().map(|x| x.elements.clone()).collect::<Vec<_>>(),
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

pub fn router() -> Router {
    build_simple_router(|route| {
        route
            .get("/dns/:database_name")
            .to_async(proxy_to_control_node_client_api);

        route.post("/init").to_async(proxy_to_control_node_client_api);

        route
            .post("/update/:address")
            .to_async(proxy_to_control_node_client_api);

        route
            .post("/delete/:address")
            .to_async(proxy_to_control_node_client_api);

        route
            .get("/subscribe")
            .with_path_extractor::<SubscribeParams>()
            .with_query_string_extractor::<SubscribeQueryParams>()
            .to_async(handle_websocket);

        route
            .get("/subscribe/:address")
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
