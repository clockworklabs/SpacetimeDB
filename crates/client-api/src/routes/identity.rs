use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::MethodRouter;
use http::header::CONTENT_TYPE;
use http::StatusCode;
use serde::{Deserialize, Serialize};

use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::Identity;

use crate::auth::{JwtAuthProvider, SpacetimeAuth, SpacetimeAuthRequired};
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
        identity: auth.claims.identity,
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
#[derive(derive_more::Into, Clone, Debug, Copy)]
pub struct IdentityForUrl(Identity);

impl From<Identity> for IdentityForUrl {
    fn from(i: Identity) -> Self {
        IdentityForUrl(i)
    }
}

impl IdentityForUrl {
    pub fn into_inner(&self) -> Identity {
        self.0
    }
}

impl<'de> serde::Deserialize<'de> for IdentityForUrl {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        <_>::deserialize(de).map(|DeserializeWrapper(b)| IdentityForUrl(Identity::from_be_byte_array(b)))
    }
}

#[derive(Deserialize)]
pub struct GetDatabasesParams {
    pub identity: IdentityForUrl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDatabasesResponse {
    pub identities: Vec<Identity>,
}

pub async fn get_databases<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Path(GetDatabasesParams { identity }): Path<GetDatabasesParams>,
) -> axum::response::Result<impl IntoResponse> {
    let identity = identity.into();
    // Linear scan for all databases that have this owner, and return their identities
    let all_dbs = ctx.get_databases().map_err(|e| {
        log::error!("Failure when retrieving databases for search: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let identities = all_dbs
        .iter()
        .filter(|db| db.owner_identity == identity)
        .map(|db| db.database_identity)
        .collect();
    Ok(axum::Json(GetDatabasesResponse { identities }))
}

#[derive(Debug, Serialize)]
pub struct WebsocketTokenResponse {
    pub token: String,
}

// This endpoint takes a token from a client and sends a newly signed token with a 60s expiry.
// Note that even if the token has a different issuer, we will sign it with our key.
// This is ok because `FullTokenValidator` checks if we signed the token before worrying about the issuer.
pub async fn create_websocket_token<S: NodeDelegate>(
    State(ctx): State<S>,
    SpacetimeAuthRequired(auth): SpacetimeAuthRequired,
) -> axum::response::Result<impl IntoResponse> {
    let expiry = Duration::from_secs(60);
    let (_, token) = auth
        .re_sign_with_expiry(ctx.jwt_auth_provider(), expiry)
        .map_err(log_and_500)?;
    // let token = encode_token_with_expiry(ctx.private_key(), auth.identity, Some(expiry)).map_err(log_and_500)?;
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

    if auth.claims.identity != identity {
        return Err(StatusCode::BAD_REQUEST.into());
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_public_key<S: NodeDelegate>(State(ctx): State<S>) -> axum::response::Result<impl IntoResponse> {
    Ok((
        [(CONTENT_TYPE, "application/pem-certificate-chain")],
        ctx.jwt_auth_provider().public_key_bytes().to_owned(),
    ))
}

/// A struct to allow customization of the `/identity` routes.
pub struct IdentityRoutes<S> {
    /// POST /identity
    pub create_post: MethodRouter<S>,
    /// GET /identity/public-key
    pub public_key_get: MethodRouter<S>,
    /// POST /identity/websocket-tocken
    pub websocket_token_post: MethodRouter<S>,
    /// GET /identity/:identity/verify
    pub verify_get: MethodRouter<S>,
    /// GET /identity/:identity/databases
    pub databases_get: MethodRouter<S>,
}

impl<S> Default for IdentityRoutes<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    fn default() -> Self {
        use axum::routing::{get, post};
        Self {
            create_post: post(create_identity::<S>),
            public_key_get: get(get_public_key::<S>),
            websocket_token_post: post(create_websocket_token::<S>),
            verify_get: get(validate_token),
            databases_get: get(get_databases::<S>),
        }
    }
}

impl<S> IdentityRoutes<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    pub fn into_router(self) -> axum::Router<S> {
        axum::Router::new()
            .route("/", self.create_post)
            .route("/public-key", self.public_key_get)
            .route("/websocket-token", self.websocket_token_post)
            .route("/:identity/verify", self.verify_get)
            .route("/:identity/databases", self.databases_get)
    }
}
