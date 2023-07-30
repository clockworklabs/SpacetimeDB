use std::sync::Arc;

use axum::extract::{FromRef, Path, Query, State};
use axum::response::IntoResponse;
use http::StatusCode;
use serde::Deserialize;
use serde_json::json;

use spacetimedb::host::EnergyQuanta;
use spacetimedb_lib::Identity;

use crate::auth::SpacetimeAuthHeader;
use crate::{log_and_500, ControlCtx, ControlNodeDelegate};

use super::identity::IdentityForUrl;

#[derive(Deserialize)]
pub struct IdentityParams {
    identity: IdentityForUrl,
}

pub async fn get_energy_balance(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Path(IdentityParams { identity }): Path<IdentityParams>,
) -> axum::response::Result<impl IntoResponse> {
    let identity = Identity::from(identity);

    // Note: Consult the write-through cache on control_budget, not the control_db directly.
    let balance = ctx
        .control_db()
        .get_energy_balance(&identity)
        .map_err(log_and_500)?
        .unwrap_or(EnergyQuanta(0));

    let response_json = json!({
        "balance": balance.0
    });

    Ok(axum::Json(response_json))
}

#[derive(Deserialize)]
pub struct SetEnergyBalanceQueryParams {
    balance: Option<i128>,
}
pub async fn set_energy_balance(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Path(IdentityParams { identity }): Path<IdentityParams>,
    Query(SetEnergyBalanceQueryParams { balance }): Query<SetEnergyBalanceQueryParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    // TODO(cloutiertyler): For the Testnet no one shall be authorized to set the energy balance
    // of an identity. Each identity will begin with a default balance and they cannot be refilled.
    // This will be a natural rate limiter until we can begin to sell energy.
    let Some(auth) = auth.auth else {
        return Err(StatusCode::UNAUTHORIZED.into());
    };

    // No one is able to be the dummy identity so this always returns unauthorized.
    if auth.identity != Identity::__dummy() {
        return Err(StatusCode::UNAUTHORIZED.into());
    }

    let identity = Identity::from(identity);

    let balance = EnergyQuanta(balance.unwrap_or(0));

    ctx.control_db()
        .set_energy_balance(identity, balance)
        .await
        .map_err(log_and_500)?;

    // Return the modified budget.
    let response_json = json!({
        "balance": balance.0,
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
        .route("/:identity", get(get_energy_balance))
        .route("/:identity", post(set_energy_balance))
}
