use std::sync::Arc;

use axum::extract::{FromRef, Path, Query, State};
use axum::response::IntoResponse;
use http::StatusCode;
use serde::Deserialize;
use serde_json::json;

use spacetimedb_lib::Identity;

use crate::{log_and_500, ControlCtx, ControlNodeDelegate};

#[derive(Deserialize)]
pub struct IdentityParams {
    identity: Identity,
}

pub async fn get_budget(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Path(IdentityParams { identity }): Path<IdentityParams>,
) -> axum::response::Result<impl IntoResponse> {
    get_budget_inner(&*ctx, &identity).await
}

#[derive(Deserialize)]
pub struct AddEnergyQueryParams {
    quanta: Option<u64>,
}
pub async fn add_energy(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Path(IdentityParams { identity }): Path<IdentityParams>,
    Query(AddEnergyQueryParams { quanta }): Query<AddEnergyQueryParams>,
) -> axum::response::Result<impl IntoResponse> {
    // TODO: we need to do authorization here. For now, just short-circuit. GOD MODE.

    if let Some(satoshi) = quanta {
        ctx.add_energy(&identity, satoshi).await.map_err(log_and_500)?;
    }
    get_budget_inner(&*ctx, &identity).await
}

async fn get_budget_inner(ctx: &dyn ControlCtx, identity: &Identity) -> axum::response::Result<impl IntoResponse> {
    let budget = ctx
        .get_energy_balance(identity)
        .await
        .map_err(log_and_500)?
        .ok_or((StatusCode::NOT_FOUND, "No budget for identity"))?;

    let response_json = json!({
        "balance": budget.balance_quanta
    });

    Ok(axum::Json(response_json))
}

pub fn router<S>() -> axum::Router<S>
where
    S: ControlNodeDelegate + Clone + 'static,
    Arc<dyn ControlCtx>: FromRef<S>,
{
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/:identity", get(get_budget))
        .route("/:identity", post(add_energy))
}
