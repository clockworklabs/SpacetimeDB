use crate::{ControlStateDelegate, NodeDelegate};
use axum::extract::State;
use axum::response::IntoResponse;
use http::StatusCode;

static VERSION: &str = env!("CARGO_PKG_VERSION");
static PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");

pub async fn health<S: ControlStateDelegate + NodeDelegate>(
    State(ctx): State<S>,
) -> axum::response::Result<impl IntoResponse> {
    let nodes: Vec<u64> = ctx
        .get_nodes()
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Couldn't connect to the control database",
            )
        })?
        .iter()
        .map(|n| n.id)
        .collect();
    let schedulable = !ctx
        .get_node_by_id(
            ctx.get_node_id()
                .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "Can't get node id"))?,
        )
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Couldn't get node info"))?
        .map(|n| n.unschedulable)
        .unwrap_or(false);

    Ok(serde_json::json!({
        "package_name": PACKAGE_NAME,
        "version": VERSION,
        "nodes": nodes,
        "schedulable": schedulable,
    })
    .to_string())
}

pub fn router<S>() -> axum::Router<S>
where
    S: ControlStateDelegate + NodeDelegate + Clone + 'static,
{
    use axum::routing::get;
    axum::Router::new().route("/", get(health::<S>))
}
