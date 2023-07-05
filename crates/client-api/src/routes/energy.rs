use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use http::StatusCode;
use serde::Deserialize;
use serde_json::json;

use spacetimedb::host::EnergyQuanta;
use spacetimedb_lib::Identity;

use crate::auth::SpacetimeAuthHeader;
use crate::{log_and_500, ControlStateDelegate, NodeDelegate};

use super::identity::IdentityForUrl;

#[derive(Deserialize)]
pub struct IdentityParams {
    identity: IdentityForUrl,
}

pub async fn get_energy_balance<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(IdentityParams { identity }): Path<IdentityParams>,
) -> axum::response::Result<impl IntoResponse> {
    let identity = Identity::from(identity);
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
        .unwrap_or(EnergyQuanta(0));

    let response_json = json!({
        // Note: balance must be returned as a string to avoid truncation.
        "balance": balance.0.to_string(),
    });

    Ok(axum::Json(response_json))
}

#[derive(Deserialize)]
pub struct SetEnergyBalanceQueryParams {
    balance: Option<String>,
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

    let balance = balance
        .map(|balance| balance.parse::<i128>())
        .transpose()
        .map_err(|err| {
            log::error!("Failed to parse balance: {:?}", err);
            StatusCode::BAD_REQUEST
        })?;
    let balance = EnergyQuanta(balance.unwrap_or(0));

    ctx.control_db()
        .set_energy_balance(identity, balance)
        .await
        .map_err(log_and_500)?;

    let response_json = json!({
        // Note: balance must be returned as a string to avoid truncation.
        "balance": balance.0.to_string(),
    });

    Ok(axum::Json(response_json))
}

pub fn router<S>() -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/:identity", get(get_energy_balance::<S>))
        .route("/:identity", post(set_energy_balance::<S>))
        .route("/:identity", put(add_energy::<S>))
}
