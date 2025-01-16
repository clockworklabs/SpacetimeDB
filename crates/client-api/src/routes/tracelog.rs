use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use http::StatusCode;
use serde::Deserialize;
use tempfile::TempDir;

use spacetimedb::address::Address;
use spacetimedb::db::Storage;
use spacetimedb::hash::hash_bytes;
use spacetimedb::host::instance_env::InstanceEnv;
use spacetimedb::host::tracelog::replay::replay_report;
use spacetimedb::host::Scheduler;
use spacetimedb::replica_context::ReplicaContext;
use spacetimedb_lib::Identity;

use crate::{log_and_500, ControlStateReadAccess, NodeDelegate};

#[derive(Deserialize)]
pub struct GetTraceParams {
    address: Address,
}
pub async fn get_tracelog<S: ControlStateReadAccess + NodeDelegate>(
    State(ctx): State<S>,
    Path(GetTraceParams { address }): Path<GetTraceParams>,
) -> axum::response::Result<impl IntoResponse> {
    let database = ctx
        .get_database_by_address(&address)
        .map_err(log_and_500)?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;
    let replica = ctx.get_leader_replica_by_database(database.id);
    let replica_id = replica.unwrap().id;

    let host = ctx.host_controller();
    let trace = host.get_trace(replica_id).await.map_err(|e| {
        log::error!("Unable to retrieve tracelog {}", e);
        (StatusCode::SERVICE_UNAVAILABLE, "Replica not ready.")
    })?;

    let trace = trace.ok_or(StatusCode::NOT_FOUND)?;

    Ok(trace)
}

#[derive(Deserialize)]
pub struct StopTraceParams {
    address: Address,
}
pub async fn stop_tracelog<S: ControlStateReadAccess + NodeDelegate>(
    State(ctx): State<S>,
    Path(StopTraceParams { address }): Path<StopTraceParams>,
) -> axum::response::Result<impl IntoResponse> {
    let database = ctx
        .get_database_by_address(&address)
        .map_err(log_and_500)?
        .ok_or((StatusCode::NOT_FOUND, "No such database."))?;
    let replica = ctx.get_leader_replica_by_database(database.id);
    let replica_id = replica.unwrap().id;

    let host = ctx.host_controller();
    host.stop_trace(replica_id).await.map_err(|e| {
        log::error!("Unable to retrieve tracelog {}", e);
        (StatusCode::SERVICE_UNAVAILABLE, "Replica not ready.")
    })?;

    Ok(())
}

pub async fn perform_tracelog_replay(body: Bytes) -> axum::response::Result<impl IntoResponse> {
    // Build out a temporary database
    let storage = Storage::Disk;
    let tmp_dir = TempDir::with_prefix("stdb_test").expect("establish tmpdir");
    let db_path = tmp_dir.path();
    let logger_path = tmp_dir.path();
    let identity = Identity::from_byte_array(hash_bytes(b"This is a fake identity.").data);
    let address = Address::from_slice(&identity.as_bytes()[0..16]);
    let replica_ctx = ReplicaContext::new(
        storage,
        0,
        0,
        false,
        identity,
        address,
        db_path.to_path_buf(),
        logger_path,
    );
    let iv = InstanceEnv::new(replica_ctx, Scheduler::dummy(&tmp_dir.path().join("scheduler")), None);

    let tx = iv.replica_ctx.relational_db.begin_mut_tx(IsolationLevel::Serializable);

    let (_, resp_body) = iv.tx.set(tx, || replay_report(&iv, &mut &body[..]));

    let resp_body = resp_body.map_err(log_and_500)?;

    Ok(axum::Json(resp_body))
}

pub fn router<S>() -> axum::Router<S>
where
    S: ControlStateReadAccess + NodeDelegate + Clone + 'static,
{
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/database/:address", get(get_tracelog::<S>))
        .route("/database/:address/stop", post(stop_tracelog::<S>))
        .route("/replay", post(perform_tracelog_replay))
}
