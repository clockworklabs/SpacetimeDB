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
use crate::nodes::control_node::control_budget;
use crate::protobuf::control_db::EnergyBudget;

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct IdentityParams {
    module_identity: String,
}

async fn get_budget(state: &mut State) -> SimpleHandlerResult {
    let IdentityParams { module_identity } = IdentityParams::take_from(state);

    // TODO: we need to do authorization here. For now, just short-circuit.

    let module_identity = match Hash::from_hex(&module_identity) {
        Ok(identity) => identity,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap());
        }
    };

    // Note: Consult the write-through cache on control_budget, not the control_db directly.
    let budget = control_budget::get_module_budget(&module_identity);
    match budget {
        None => {
            return Err(HandlerError::from(anyhow!("No budget for identity")).with_status(StatusCode::NOT_FOUND));
        }
        Some(budget) => {
            let response_json = json!({
                "balance": budget.balance_quanta,
                "default_reducer_maximum": budget.default_reducer_maximum_quanta
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
struct SetBudgetQueryParams {
    balance: Option<i64>,
    default_maximum: Option<u64>,
}
async fn set_budget(state: &mut State) -> SimpleHandlerResult {
    let IdentityParams { module_identity } = IdentityParams::take_from(state);

    let SetBudgetQueryParams {
        balance,
        default_maximum,
    } = SetBudgetQueryParams::take_from(state);

    // TODO: we need to do authorization here. For now, just short-circuit. GOD MODE.

    let module_identity = match Hash::from_hex(&module_identity) {
        Ok(identity) => identity,
        Err(e) => {
            log::error!("Invalid identity: {}: {}", module_identity, e);
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap());
        }
    };

    // We're only updating part of the budget, so we need to retrieve first and alter only the
    // parts we're updating
    // If there's no existing budget, create new with sensible defaults.
    let budget = control_budget::get_module_budget(&module_identity);
    let budget = match budget {
        Some(mut budget) => {
            if balance.is_some() {
                budget.balance_quanta = balance.unwrap();
            }
            if default_maximum.is_some() {
                budget.default_reducer_maximum_quanta = default_maximum.unwrap();
            }
            budget
        }
        None => {
            EnergyBudget {
                module_identity: Vec::from(module_identity.data),
                balance_quanta: balance.unwrap_or(0),
                default_reducer_maximum_quanta: default_maximum.unwrap_or(1_000_000_000), /* TODO: this should be a global constant */
            }
        }
    };

    control_budget::set_module_budget(&module_identity, &budget).await;

    // Return the modified budget.
    let response_json = json!({
        "balance": budget.balance_quanta,
        "default_reducer_maximum": budget.default_reducer_maximum_quanta
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
            .get("/:module_identity")
            .with_path_extractor::<IdentityParams>()
            .to_async_borrowing(get_budget);

        route
            .post("/:module_identity")
            .with_path_extractor::<IdentityParams>()
            .with_query_string_extractor::<SetBudgetQueryParams>()
            .to_async_borrowing(set_budget);
    })
}
