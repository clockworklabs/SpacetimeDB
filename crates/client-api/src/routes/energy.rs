use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use http::StatusCode;
use serde::Deserialize;
use serde_json::json;

use spacetimedb_lib::Identity;

use crate::{log_and_500, ControlStateDelegate, NodeDelegate};

#[derive(Deserialize)]
pub struct IdentityParams {
    identity: Identity,
}

pub async fn get_budget<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(IdentityParams { identity }): Path<IdentityParams>,
) -> axum::response::Result<impl IntoResponse> {
    get_budget_inner(ctx, &identity)
}

#[derive(Deserialize)]
pub struct AddEnergyQueryParams {
    quanta: Option<u64>,
}
pub async fn add_energy<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(IdentityParams { identity }): Path<IdentityParams>,
    Query(AddEnergyQueryParams { quanta }): Query<AddEnergyQueryParams>,
) -> axum::response::Result<impl IntoResponse> {
    // TODO: we need to do authorization here. For now, just short-circuit. GOD MODE.

    if let Some(satoshi) = quanta {
        ctx.add_energy(&identity, satoshi).await.map_err(log_and_500)?;
    }
    get_budget_inner(ctx, &identity)
}

fn get_budget_inner(ctx: impl ControlStateDelegate, identity: &Identity) -> axum::response::Result<impl IntoResponse> {
    let budget = ctx
        .get_energy_balance(identity)
        .map_err(log_and_500)?
        .ok_or((StatusCode::NOT_FOUND, "No budget for identity"))?;

    let response_json = json!({
        "balance": budget.balance_quanta
    });

    Ok(axum::Json(response_json))
}

pub fn router<S>() -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/:identity", get(get_budget::<S>))
        .route("/:identity", post(add_energy::<S>))
}
