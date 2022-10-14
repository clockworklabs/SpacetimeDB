use anyhow::anyhow;
use gotham::handler::HandlerError;
use gotham::{
    handler::SimpleHandlerResult,
    prelude::*,
    router::{build_simple_router, Router},
    state::State,
};
use hyper::{Body, Response, StatusCode};
use serde::Deserialize;
use serde_json::json;

use crate::hash::Hash;
use crate::nodes::control_node::budget_controller;
use crate::protobuf::control_db::EnergyBalance;

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct IdentityParams {
    identity: String,
}

async fn get_budget(state: &mut State) -> SimpleHandlerResult {
    let IdentityParams { identity } = IdentityParams::take_from(state);

    // TODO: we need to do authorization here. For now, just short-circuit.

    let identity = match Hash::from_hex(&identity) {
        Ok(identity) => identity,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap());
        }
    };

    // Note: Consult the write-through cache on control_budget, not the control_db directly.
    let budget = budget_controller::get_identity_energy_balance(&identity);
    match budget {
        None => {
            return Err(HandlerError::from(anyhow!("No budget for identity")).with_status(StatusCode::NOT_FOUND));
        }
        Some(budget) => {
            let response_json = json!({
                "balance": budget.balance_quanta
            });

            let response = Response::builder()
                .status(StatusCode::OK)
                .body(Body::from(response_json.to_string()))
                .unwrap();

            Ok(response)
        }
    }
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct SetEnergyBalanceQueryParams {
    balance: Option<i64>,
}
async fn set_energy_balance(state: &mut State) -> SimpleHandlerResult {
    let IdentityParams { identity } = IdentityParams::take_from(state);

    let SetEnergyBalanceQueryParams { balance } = SetEnergyBalanceQueryParams::take_from(state);

    // TODO: we need to do authorization here. For now, just short-circuit. GOD MODE.

    let identity = match Hash::from_hex(&identity) {
        Ok(identity) => identity,
        Err(e) => {
            log::error!("Invalid identity: {}: {}", identity, e);
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap());
        }
    };

    // We're only updating part of the budget, so we need to retrieve first and alter only the
    // parts we're updating
    // If there's no existing budget, create new with sensible defaults.
    let budget = budget_controller::get_identity_energy_balance(&identity);
    let budget = match budget {
        Some(mut budget) => {
            if balance.is_some() {
                budget.balance_quanta = balance.unwrap();
            }
            budget
        }
        None => EnergyBalance {
            identity: Vec::from(identity.data),
            balance_quanta: balance.unwrap_or(0),
        },
    };

    budget_controller::set_identity_energy_balance(&identity, &budget).await;

    // Return the modified budget.
    let response_json = json!({
        "balance": budget.balance_quanta,
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(response_json.to_string()))
        .unwrap();

    Ok(response)
}

pub fn router() -> Router {
    build_simple_router(|route| {
        route
            .get("/:identity")
            .with_path_extractor::<IdentityParams>()
            .to_async_borrowing(get_budget);

        route
            .post("/:identity")
            .with_path_extractor::<IdentityParams>()
            .with_query_string_extractor::<SetEnergyBalanceQueryParams>()
            .to_async_borrowing(set_energy_balance);
    })
}
