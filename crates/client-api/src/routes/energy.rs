use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use http::StatusCode;
use serde::{Deserialize, Serialize};

use spacetimedb::energy::EnergyQuanta;
use spacetimedb_lib::Identity;

use crate::auth::SpacetimeAuthRequired;
use crate::{log_and_500, ControlStateDelegate, NodeDelegate};

use super::identity::IdentityForUrl;

#[derive(Deserialize)]
pub struct IdentityParams {
    identity: IdentityForUrl,
}

// TODO: do we want to require auth on this?
pub async fn get_energy_balance<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(IdentityParams { identity }): Path<IdentityParams>,
) -> axum::response::Result<impl IntoResponse> {
    let identity = Identity::from(identity);
    get_budget_inner(ctx, &identity)
}

#[serde_with::serde_as]
#[derive(Serialize)]
struct BalanceResponse {
    // Note: balance must be returned as a string to avoid truncation.
    #[serde_as(as = "serde_with::DisplayFromStr")]
    balance: i128,
}

#[derive(Deserialize)]
pub struct AddEnergyQueryParams {
    amount: Option<String>,
}
pub async fn add_energy<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Query(AddEnergyQueryParams { amount }): Query<AddEnergyQueryParams>,
    SpacetimeAuthRequired(auth): SpacetimeAuthRequired,
) -> axum::response::Result<impl IntoResponse> {
    // Nb.: Negative amount withdraws
    let amount = amount.map(|s| s.parse::<u128>()).transpose().map_err(|e| {
        log::error!("Failed to parse amount: {e:?}");
        StatusCode::BAD_REQUEST
    })?;

    if let Some(satoshi) = amount {
        ctx.add_energy(&auth.identity, EnergyQuanta::new(satoshi))
            .await
            .map_err(log_and_500)?;
    }

    // TODO: is this guaranteed to pull the updated balance?
    let balance = ctx
        .get_energy_balance(&auth.identity)
        .map_err(log_and_500)?
        .map_or(0, |quanta| quanta.get());

    Ok(axum::Json(BalanceResponse { balance }))
}

fn get_budget_inner(ctx: impl ControlStateDelegate, identity: &Identity) -> axum::response::Result<impl IntoResponse> {
    let balance = ctx
        .get_energy_balance(identity)
        .map_err(log_and_500)?
        .map_or(0, |quanta| quanta.get());

    Ok(axum::Json(BalanceResponse { balance }))
}

#[derive(Deserialize)]
pub struct SetEnergyBalanceQueryParams {
    balance: Option<String>,
}
pub async fn set_energy_balance<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(IdentityParams { identity }): Path<IdentityParams>,
    Query(SetEnergyBalanceQueryParams { balance }): Query<SetEnergyBalanceQueryParams>,
    SpacetimeAuthRequired(auth): SpacetimeAuthRequired,
) -> axum::response::Result<impl IntoResponse> {
    // TODO(cloutiertyler): For the Testnet no one shall be authorized to set the energy balance
    // of an identity. Each identity will begin with a default balance and they cannot be refilled.
    // This will be a natural rate limiter until we can begin to sell energy.

    // No one is able to be the dummy identity so this always returns unauthorized.
    if auth.identity != Identity::__dummy() {
        return Err(StatusCode::UNAUTHORIZED.into());
    }

    let identity = Identity::from(identity);

    let desired_balance = balance
        .map(|balance| balance.parse::<i128>())
        .transpose()
        .map_err(|err| {
            log::error!("Failed to parse balance: {:?}", err);
            StatusCode::BAD_REQUEST
        })?
        .unwrap_or(0);
    let current_balance = ctx
        .get_energy_balance(&identity)
        .map_err(log_and_500)?
        .map_or(0, |quanta| quanta.get());

    // TODO: this is a race condition waiting to happen. have a set_balance method on ControlStateDelegate
    let delta = EnergyQuanta::new(desired_balance.abs_diff(current_balance));
    if desired_balance > current_balance {
        ctx.add_energy(&identity, delta).await.map_err(log_and_500)?;
    } else {
        ctx.withdraw_energy(&identity, delta).await.map_err(log_and_500)?;
    }

    Ok(axum::Json(BalanceResponse {
        balance: desired_balance,
    }))
}

pub fn router<S>() -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    use axum::routing::get;
    axum::Router::new().route(
        "/:identity",
        get(get_energy_balance::<S>)
            .put(set_energy_balance::<S>)
            .post(add_energy::<S>),
    )
}
