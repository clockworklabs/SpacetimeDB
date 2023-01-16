use email_address;
use gotham::{
    handler::SimpleHandlerResult,
    prelude::*,
    router::{build_simple_router, Router},
    state::State,
};
use hyper::{header::AUTHORIZATION, Body, HeaderMap, Response, StatusCode};
use serde::{Deserialize, Serialize};
use spacetimedb::address::Address;
use spacetimedb::control_db::CONTROL_DB;
use spacetimedb::{
    auth::{
        get_creds_from_header,
        identity::{decode_token, encode_token},
        invalid_token_res,
    },
    hash::Hash,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateIdentityResponse {
    identity: String,
    token: String,
}

async fn create_identity(_state: &mut State) -> SimpleHandlerResult {
    let identity = CONTROL_DB.alloc_spacetime_identity().await?;
    let token = encode_token(identity)?;

    let identity_response = CreateIdentityResponse {
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

#[derive(Debug, Clone, Serialize)]
struct GetIdentityResponse {
    identities: Vec<GetIdentityResponseEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct GetIdentityResponseEntry {
    identity: String,
    email: String,
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct GetIdentityQueryParams {
    email: Option<String>,
}
async fn get_identity(state: &mut State) -> SimpleHandlerResult {
    let GetIdentityQueryParams { email } = GetIdentityQueryParams::take_from(state);

    let lookup = match email {
        None => None,
        Some(email) => {
            let im = CONTROL_DB.get_identities_for_email(email.as_str());
            match im {
                Ok(identities) => {
                    if identities.is_empty() {
                        None
                    } else {
                        let mut response = GetIdentityResponse {
                            identities: Vec::<GetIdentityResponseEntry>::new(),
                        };

                        for identity_email in identities {
                            response.identities.push(GetIdentityResponseEntry {
                                identity: Hash::from_slice(&identity_email.identity[..]).to_hex(),
                                email: identity_email.email,
                            })
                        }
                        Some(response)
                    }
                }
                Err(_e) => {
                    return Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap());
                }
            }
        }
    };
    match lookup {
        None => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap()),
        Some(identity_response) => {
            let identity_json = serde_json::to_string(&identity_response).unwrap();
            Ok(Response::builder()
                .status(StatusCode::OK)
                .body(Body::from(identity_json))
                .unwrap())
        }
    }
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
    let SetEmailParams { identity } = SetEmailParams::take_from(state);
    let SetEmailQueryParams { email } = SetEmailQueryParams::take_from(state);
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
        }
    };

    CONTROL_DB
        .associate_email_spacetime_identity(&identity, &email)
        .await
        .unwrap();

    let res = Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap();

    Ok(res)
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct GetDatabasesParams {
    identity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetDatabasesResponse {
    addresses: Vec<String>,
}

async fn get_databases(state: &mut State) -> SimpleHandlerResult {
    let GetDatabasesParams { identity } = GetDatabasesParams::take_from(state);

    let res = match identity {
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
        Some(identity) => {
            let identity = match Hash::from_hex(identity.as_str()) {
                Ok(identity) => identity,
                Err(_) => {
                    return Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .unwrap())
                }
            };

            // Linear scan for all databases that have this identity, and return their addresses
            let all_dbs = CONTROL_DB.get_databases().await;
            match all_dbs {
                Ok(all_dbs) => {
                    let matching_dbs = all_dbs.into_iter().filter(|db| db.identity == identity.data);
                    let addresses = matching_dbs.map(|db| Address::from_slice(&db.address[..]).to_hex());
                    let response = GetDatabasesResponse {
                        addresses: addresses.collect(),
                    };
                    let json = serde_json::to_string(&response).unwrap();
                    let body = Body::from(json);
                    Response::builder().status(StatusCode::OK).body(body).unwrap()
                }
                Err(e) => {
                    log::error!("Failure when retrieving databases for search: {}", e);
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap()
                }
            }
        }
    };

    Ok(res)
}

pub fn router() -> Router {
    build_simple_router(|route| {
        route
            .get("/")
            .with_query_string_extractor::<GetIdentityQueryParams>()
            .to_async_borrowing(get_identity);

        route.post("/").to_async_borrowing(create_identity);

        route
            .post("/:identity/set-email")
            .with_path_extractor::<SetEmailParams>()
            .with_query_string_extractor::<SetEmailQueryParams>()
            .to_async_borrowing(set_email);

        route
            .get("/:identity/databases")
            .with_path_extractor::<GetDatabasesParams>()
            .to_async_borrowing(get_databases);
    })
}
