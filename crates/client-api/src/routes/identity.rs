use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use http::header::CONTENT_TYPE;
use http::StatusCode;
use serde::{Deserialize, Serialize};

use spacetimedb::auth::identity::encode_token_with_expiry;
use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::{Address, Identity};

use crate::auth::{SpacetimeAuth, SpacetimeAuthRequired};
use crate::{log_and_500, ControlStateDelegate, NodeDelegate};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIdentityResponse {
    identity: Identity,
    token: String,
}

pub async fn create_identity<S: ControlStateDelegate + NodeDelegate>(
    State(ctx): State<S>,
) -> axum::response::Result<impl IntoResponse> {
    let auth = SpacetimeAuth::alloc(&ctx).await?;

    let identity_response = CreateIdentityResponse {
        identity: auth.identity,
        token: auth.creds.token().to_owned(),
    };
    Ok(axum::Json(identity_response))
}

/// A version of `Identity` appropriate for URL de/encoding.
///
/// Because `Identity` is represented in SATS as a `ProductValue`,
/// its serialized format is somewhat gnarly.
/// When URL-encoding identities, we want to use only the hex string,
/// without wrapping it in a `ProductValue`.
/// This keeps our routes pretty, like `/identity/<64 hex chars>/set-email`.
///
/// This newtype around `Identity` implements `Deserialize`
/// directly from the inner identity bytes,
/// without the enclosing `ProductValue` wrapper.
#[derive(derive_more::Into)]
pub struct IdentityForUrl(Identity);

impl<'de> serde::Deserialize<'de> for IdentityForUrl {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        <_>::deserialize(de).map(|DeserializeWrapper(b)| IdentityForUrl(Identity::from_byte_array(b)))
    }
}

#[derive(Deserialize)]
pub struct GetDatabasesParams {
    identity: IdentityForUrl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDatabasesResponse {
    addresses: Vec<Address>,
}

pub async fn get_databases<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(GetDatabasesParams { identity }): Path<GetDatabasesParams>,
) -> axum::response::Result<impl IntoResponse> {
    let identity = identity.into();
    // Linear scan for all databases that have this identity, and return their addresses
    let all_dbs = ctx.get_databases().map_err(|e| {
        log::error!("Failure when retrieving databases for search: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let addresses = all_dbs
        .iter()
        .filter(|db| db.owner_identity == identity)
        .map(|db| db.address)
        .collect();
    Ok(axum::Json(GetDatabasesResponse { addresses }))
}

#[derive(Debug, Serialize)]
pub struct WebsocketTokenResponse {
    pub token: String,
}

pub async fn create_websocket_token<S: NodeDelegate>(
    State(ctx): State<S>,
    SpacetimeAuthRequired(auth): SpacetimeAuthRequired,
) -> axum::response::Result<impl IntoResponse> {
    let expiry = Duration::from_secs(60);
    let token = encode_token_with_expiry(ctx.private_key(), auth.identity, Some(expiry)).map_err(log_and_500)?;
    Ok(axum::Json(WebsocketTokenResponse { token }))
}

#[derive(Deserialize)]
pub struct ValidateTokenParams {
    identity: IdentityForUrl,
}

pub async fn validate_token(
    Path(ValidateTokenParams { identity }): Path<ValidateTokenParams>,
    SpacetimeAuthRequired(auth): SpacetimeAuthRequired,
) -> axum::response::Result<impl IntoResponse> {
    let identity = Identity::from(identity);

    if auth.identity != identity {
        return Err(StatusCode::BAD_REQUEST.into());
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_public_key<S: NodeDelegate>(State(ctx): State<S>) -> axum::response::Result<impl IntoResponse> {
    Ok((
        [(CONTENT_TYPE, "application/pem-certificate-chain")],
        ctx.public_key_bytes().to_owned(),
    ))
}

pub fn router<S>(_: S) -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/", post(create_identity::<S>))
        .route("/public-key", get(get_public_key::<S>))
        .route("/websocket_token", post(create_websocket_token::<S>))
        .route("/:identity/verify", get(validate_token))
        .route("/:identity/databases", get(get_databases::<S>))
}
