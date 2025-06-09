use axum::response::IntoResponse;
use spacetimedb_lib::{sats, Timestamp};

use crate::NodeDelegate;

/// Returns the database's view of the current time,
/// as a SATS-JSON encoded [`Timestamp`].
async fn get_timestamp() -> impl IntoResponse {
    axum::Json(sats::serde::SerdeWrapper(Timestamp::now())).into_response()
}

/// The internal router is for routes which are early in design,
/// and may incompatibly change or be removed without a major version bump.
pub fn router<S>() -> axum::Router<S>
where
    S: NodeDelegate + Clone + 'static,
{
    use axum::routing::get;

    axum::Router::new().route("/timestamp", get(get_timestamp))
}
