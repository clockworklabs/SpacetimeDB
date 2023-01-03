use std::collections::HashMap;
use std::io::BufReader;
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use gotham::handler::{HandlerError, SimpleHandlerResult};
use gotham::prelude::FromState;
use gotham::prelude::{DefineSingleRoute, DrawRoutes};
use gotham::router::{build_simple_router, Router};
use gotham::state::State;
use gotham_derive::{StateData, StaticResponseExtender};
use hyper::{Body, Response, StatusCode};
use serde::Deserialize;
use spacetimedb::control_db;
use tempdir::TempDir;

use spacetimedb::address::Address;
use spacetimedb::hash::hash_bytes;
use spacetimedb::host::host_controller;
use spacetimedb::host::instance_env::InstanceEnv;
use spacetimedb::host::tracelog::replay::replay_report;
use spacetimedb::protobuf::control_db::HostType;
use spacetimedb::worker_database_instance::WorkerDatabaseInstance;

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct GetTraceParams {
    address: String,
}
async fn get_tracelog(state: &mut State) -> SimpleHandlerResult {
    let GetTraceParams { address } = GetTraceParams::take_from(state);
    let address = Address::from_hex(&address)?;
    let database = match control_db::get_database_by_address(&address).await.unwrap() {
        Some(database) => database,
        None => return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND)),
    };
    let database_instance = control_db::get_leader_database_instance_by_database(database.id).await;
    let instance_id = database_instance.unwrap().id;

    let host = host_controller::get_host();
    let trace = match host.get_trace(instance_id).await {
        Ok(trace) => trace,
        Err(e) => {
            log::error!("Unable to retrieve tracelog {}", e);
            return Err(HandlerError::from(anyhow!("Database instance not ready."))
                .with_status(StatusCode::SERVICE_UNAVAILABLE));
        }
    };

    let response = match trace {
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
        Some(trace) => Response::builder()
            .status(StatusCode::OK)
            .body(Body::from(trace))
            .unwrap(),
    };

    Ok(response)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct StopTraceParams {
    address: String,
}
async fn stop_tracelog(state: &mut State) -> SimpleHandlerResult {
    let StopTraceParams { address } = StopTraceParams::take_from(state);
    let address = Address::from_hex(&address)?;
    let database = match control_db::get_database_by_address(&address).await? {
        Some(database) => database,
        None => return Err(HandlerError::from(anyhow!("No such database.")).with_status(StatusCode::NOT_FOUND)),
    };
    let database_instance = control_db::get_leader_database_instance_by_database(database.id).await;
    let instance_id = database_instance.unwrap().id;

    let host = host_controller::get_host();
    match host.stop_trace(instance_id).await {
        Ok(trace) => trace,
        Err(e) => {
            log::error!("Unable to stop tracelog {}", e);
            return Err(HandlerError::from(anyhow!("Database instance not ready."))
                .with_status(StatusCode::SERVICE_UNAVAILABLE));
        }
    };

    Ok(Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap())
}

async fn perform_tracelog_replay(state: &mut State) -> SimpleHandlerResult {
    let body = state.borrow_mut::<Body>();
    let data = hyper::body::to_bytes(body).await;
    let data = match data {
        Ok(data) => data,
        Err(_) => return Err(HandlerError::from(anyhow!("Invalid request body")).with_status(StatusCode::BAD_REQUEST)),
    };
    let trace_log_bytes = data.to_vec();

    // Build out a temporary database
    let tmp_dir = TempDir::new("stdb_test").expect("establish tmpdir");
    let db_path = tmp_dir.path();
    let logger_path = tmp_dir.path();
    let identity = hash_bytes(b"This is a fake identity.");
    let address = Address::from_slice(&identity.as_slice()[0..16]);
    let wdi = WorkerDatabaseInstance::new(0, 0, HostType::Wasmer, false, identity, address, db_path, logger_path);
    let itx = Arc::new(Mutex::new(HashMap::new()));
    let iv = InstanceEnv::new(0, wdi, itx, None);

    let tx = iv.worker_database_instance.relational_db.begin_tx();
    iv.instance_tx_map.lock().unwrap().insert(0, tx);
    let mut reader = BufReader::new(&trace_log_bytes[..]);

    match replay_report(&iv, &mut reader) {
        Ok(resp_body) => {
            let res = match serde_json::to_string(&resp_body) {
                Ok(j) => Response::builder().status(StatusCode::OK).body(Body::from(j)).unwrap(),
                Err(e) => {
                    log::error!("Unable to serialize tracelog response: {}", e);
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap()
                }
            };
            Ok(res)
        }
        Err(e) => return Err(HandlerError::from(e).with_status(StatusCode::INTERNAL_SERVER_ERROR)),
    }
}

pub fn router() -> Router {
    build_simple_router(|route| {
        route
            .get("/database/:address")
            .with_path_extractor::<GetTraceParams>()
            .to_async_borrowing(get_tracelog);

        route
            .post("/database/:address/stop")
            .with_path_extractor::<StopTraceParams>()
            .to_async_borrowing(stop_tracelog);

        route.post("/replay").to_async_borrowing(perform_tracelog_replay);
    })
}
