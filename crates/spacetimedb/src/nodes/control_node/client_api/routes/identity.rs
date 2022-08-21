use crate::{auth::{identity::{encode_token, decode_token}, get_creds_from_header, invalid_token_res}, nodes::control_node::control_db, hash::Hash};
use gotham::{
    handler::SimpleHandlerResult,
    prelude::*,
    router::{build_simple_router, Router},
    state::State,
};
use hyper::{Body, Response, StatusCode, HeaderMap, header::AUTHORIZATION};
use serde::{Deserialize, Serialize};
use email_address;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdentityResponse {
    identity: String,
    token: String,
}

async fn get_identity(_state: &mut State) -> SimpleHandlerResult {
    let identity = control_db::alloc_spacetime_identity().await?;
    let token = encode_token(identity)?;

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

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct SetEmailParams {
    identity: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct SetEmailQueryParams {
    email: String,
}

async fn set_email(state: &mut State) -> SimpleHandlerResult {
    let SetEmailParams {
        identity,
    } = SetEmailParams::take_from(state);
    let SetEmailQueryParams {
        email,
    } = SetEmailQueryParams::take_from(state);
    let headers = state.borrow::<HeaderMap>();
    let auth_header = headers.get(AUTHORIZATION);
    let (_caller_identity, caller_identity_token) = if let Some(auth_header) = auth_header {
        // Validate the credentials of this connection
        match get_creds_from_header(auth_header) {
            Ok(v) => v,
            Err(_) => return Ok(invalid_token_res()),
        }
    } else {
        return Ok(invalid_token_res());
    };

    let token = decode_token(&caller_identity_token)?;

    if token.claims.hex_identity != identity {
        return Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::empty())
            .unwrap());
    }

    // Basic RFC compliant sanity checking
    if !email_address::EmailAddress::is_valid(&email) {
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .unwrap());
    }

    let identity = match Hash::from_hex(&identity) {
        Ok(identity) => identity,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::empty())
                .unwrap());
        },
    };

    control_db::associate_email_spacetime_identity(&identity, &email).await.unwrap();

    let res = Response::builder()
        .status(StatusCode::OK)
        .body(Body::empty())
        .unwrap();

    Ok(res)
}

pub fn router() -> Router {
    build_simple_router(|route| {
        route.post("/").to_async_borrowing(get_identity);
        route.post("/:identity/set-email")
            .with_path_extractor::<SetEmailParams>()
            .with_query_string_extractor::<SetEmailQueryParams>()
            .to_async_borrowing(set_email);
    })
}
