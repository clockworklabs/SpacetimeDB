use super::database::worker_ctx_find_database;
use crate::{routes::database::NO_SUCH_DATABASE, util::NameOrIdentity, ControlStateDelegate};
use axum::{
    extract::{Path, State},
    response::IntoResponse,
};
use spacetimedb_lib::{sats, Timestamp};

#[derive(serde::Deserialize)]
pub struct TimestampParams {
    name_or_identity: NameOrIdentity,
}

/// Returns the database's view of the current time,
/// as a SATS-JSON encoded [`Timestamp`].
///
/// Takes a particular database's [`NameOrIdentity`] as an argument
/// because in a clusterized SpacetimeDB-cloud deployment,
/// this request will be routed to the node running the requested database.
async fn get_timestamp<S: ControlStateDelegate>(
    State(worker_ctx): State<S>,
    Path(TimestampParams { name_or_identity }): Path<TimestampParams>,
) -> axum::response::Result<impl IntoResponse> {
    let db_identity = name_or_identity.resolve(&worker_ctx).await?;

    let _database = worker_ctx_find_database(&worker_ctx, &db_identity)
        .await?
        .ok_or_else(|| {
            log::error!("Could not find database: {}", db_identity.to_hex());
            NO_SUCH_DATABASE
        })?;

    Ok(axum::Json(sats::serde::SerdeWrapper(Timestamp::now())).into_response())
}

/// The internal router is for routes which are early in design,
/// and may incompatibly change or be removed without a major version bump.
pub fn router<S>() -> axum::Router<S>
where
    S: ControlStateDelegate + Clone + 'static,
{
    use axum::routing::get;

    axum::Router::<S>::new().route("/database/:name_or_identity/timestamp", get(get_timestamp::<S>))
}
