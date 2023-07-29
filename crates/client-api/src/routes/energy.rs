use std::sync::Arc;

use axum::extract::{FromRef, Path, Query, State};
use axum::response::IntoResponse;
use http::StatusCode;
use serde::Deserialize;
use serde_json::json;

use spacetimedb::messages::control_db::EnergyBalance;
use spacetimedb_lib::Identity;

use crate::{log_and_500, ControlCtx, ControlNodeDelegate};

use super::identity::IdentityForUrl;

#[derive(Deserialize)]
pub struct IdentityParams {
    identity: IdentityForUrl,
}

pub async fn get_budget(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Path(IdentityParams { identity }): Path<IdentityParams>,
) -> axum::response::Result<impl IntoResponse> {
    // TODO: we need to do authorization here. For now, just short-circuit.

    let identity = Identity::from(identity);

    // Note: Consult the write-through cache on control_budget, not the control_db directly.
    let budget = ctx
        .control_db()
        .get_energy_balance(&identity)
        .await
        .map_err(log_and_500)?
        .ok_or((StatusCode::NOT_FOUND, "No budget for identity"))?;

    let response_json = json!({
        "balance": budget.balance_quanta
    });

    Ok(axum::Json(response_json))
}

#[derive(Deserialize)]
pub struct SetEnergyBalanceQueryParams {
    balance: Option<i64>,
}
pub async fn set_energy_balance(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Path(IdentityParams { identity }): Path<IdentityParams>,
    Query(SetEnergyBalanceQueryParams { balance }): Query<SetEnergyBalanceQueryParams>,
) -> axum::response::Result<impl IntoResponse> {
    // TODO: we need to do authorization here. For now, just short-circuit. GOD MODE.

    let identity = Identity::from(identity);

    // We're only updating part of the budget, so we need to retrieve first and alter only the
    // parts we're updating
    // If there's no existing budget, create new with sensible defaults.
    let budget = ctx
        .control_db()
        .get_energy_balance(&identity)
        .await
        .map_err(log_and_500)?;
    let mut budget = budget.unwrap_or(EnergyBalance {
        identity,
        balance_quanta: 0,
    });

    if let Some(balance) = balance {
        budget.balance_quanta = balance
    }

    ctx.control_db()
        .set_energy_balance(&identity, &budget)
        .map_err(log_and_500)?;

    // Return the modified budget.
    let response_json = json!({
        "balance": budget.balance_quanta,
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
        .route("/:identity", post(set_energy_balance))
}
