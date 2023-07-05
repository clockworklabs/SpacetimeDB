use std::sync::Arc;

use axum::extract::{FromRef, Path, Query, State};
use axum::response::IntoResponse;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use spacetimedb::auth::identity::encode_token_with_expiry;
use spacetimedb_lib::Identity;

use crate::auth::{SpacetimeAuth, SpacetimeAuthHeader};
use crate::{log_and_500, ControlCtx, ControlNodeDelegate};

#[derive(Deserialize)]
pub struct CreateIdentityQueryParams {
    email: Option<email_address::EmailAddress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIdentityResponse {
    identity: String,
    token: String,
}

pub async fn create_identity(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Query(CreateIdentityQueryParams { email }): Query<CreateIdentityQueryParams>,
) -> axum::response::Result<impl IntoResponse> {
    let auth = SpacetimeAuth::alloc(&*ctx).await?;
    if let Some(email) = email {
        ctx.add_email(&auth.identity, email.as_str()).await.unwrap();
    }

    let identity_response = CreateIdentityResponse {
        identity: auth.identity.to_hex(),
        token: auth.creds.token().to_owned(),
    };
    Ok(axum::Json(identity_response))
}

#[derive(Debug, Clone, Serialize)]
pub struct GetIdentityResponse {
    identities: Vec<GetIdentityResponseEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetIdentityResponseEntry {
    identity: String,
    email: String,
}

#[derive(Deserialize)]
pub struct GetIdentityQueryParams {
    email: Option<String>,
}
pub async fn get_identity(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Query(GetIdentityQueryParams { email }): Query<GetIdentityQueryParams>,
) -> axum::response::Result<impl IntoResponse> {
    let lookup = match email {
        None => None,
        Some(email) => {
            let identities = ctx
                .get_identities_for_email(email.as_str())
                .await
                .map_err(log_and_500)?;
            if identities.is_empty() {
                None
            } else {
                let mut response = GetIdentityResponse {
                    identities: Vec::<GetIdentityResponseEntry>::new(),
                };

                for identity_email in identities {
                    response.identities.push(GetIdentityResponseEntry {
                        identity: identity_email.identity.to_hex(),
                        email: identity_email.email,
                    })
                }
                Some(response)
            }
        }
    };
    let identity_response = lookup.ok_or(StatusCode::NOT_FOUND)?;
    Ok(axum::Json(identity_response))
}

#[derive(Deserialize)]
pub struct SetEmailParams {
    identity: Identity,
}

#[derive(Deserialize)]
pub struct SetEmailQueryParams {
    email: email_address::EmailAddress,
}

pub async fn set_email(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Path(SetEmailParams { identity }): Path<SetEmailParams>,
    Query(SetEmailQueryParams { email }): Query<SetEmailQueryParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    let auth = auth.get().ok_or(StatusCode::BAD_REQUEST)?;

    if auth.identity != identity {
        return Err(StatusCode::UNAUTHORIZED.into());
    }
    ctx.add_email(&identity, email.as_str()).await.map_err(log_and_500)?;

    Ok(())
}

#[derive(Deserialize)]
pub struct GetDatabasesParams {
    identity: Identity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDatabasesResponse {
    addresses: Vec<String>,
}

pub async fn get_databases(
    State(ctx): State<Arc<dyn ControlCtx>>,
    Path(GetDatabasesParams { identity }): Path<GetDatabasesParams>,
) -> axum::response::Result<impl IntoResponse> {
    // Linear scan for all databases that have this identity, and return their addresses
    let all_dbs = ctx.get_databases().await.map_err(|e| {
        log::error!("Failure when retrieving databases for search: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let matching_dbs = all_dbs.into_iter().filter(|db| db.identity == identity);
    let addresses = matching_dbs.map(|db| db.address.to_hex());
    let response = GetDatabasesResponse {
        addresses: addresses.collect(),
    };
    Ok(axum::Json(response))
}

#[derive(Debug, Serialize)]
pub struct WebsocketTokenResponse {
    token: String,
}

pub async fn create_websocket_token(
    State(ctx): State<Arc<dyn ControlCtx>>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    match auth.auth {
        Some(auth) => {
            let token = encode_token_with_expiry(ctx.private_key(), auth.identity, Some(60)).map_err(log_and_500)?;
            Ok(axum::Json(WebsocketTokenResponse { token }))
        }
        None => Err(StatusCode::UNAUTHORIZED)?,
    }
}

pub fn router<S>() -> axum::Router<S>
where
    S: ControlNodeDelegate + Clone + 'static,
    Arc<dyn ControlCtx>: FromRef<S>,
{
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/", get(get_identity).post(create_identity))
        .route("/websocket_token", post(create_websocket_token))
        .route("/:identity/set-email", post(set_email))
        .route("/:identity/databases", get(get_databases))
}
