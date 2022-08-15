use gotham::{
    handler::SimpleHandlerResult,
    prelude::*,
    router::{build_simple_router, Router},
    state::State,
};
use hyper::{Body, Response, StatusCode};
use serde::{Deserialize, Serialize};

use crate::api::spacetime_identity;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdentityResponse {
    identity: String,
    token: String,
}

async fn get_identity(_state: &mut State) -> SimpleHandlerResult {
    let (identity, token) = spacetime_identity().await.unwrap();

    let identity_response = IdentityResponse {
        identity: identity.to_hex(),
        token,
    };
    let json = serde_json::to_string(&identity_response).unwrap();

    let res = Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(json))
        .unwrap();

    Ok(res)
}

pub fn router() -> Router {
    build_simple_router(|route| {
        route.get("/").to_async_borrowing(get_identity);
    })
}
