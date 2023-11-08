use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use serde::{Deserialize, Serialize};

use spacetimedb::auth::identity::encode_token_with_expiry;
use spacetimedb::messages::control_db::IdentityEmail;
use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::{Address, Identity};

use crate::auth::{SpacetimeAuth, SpacetimeAuthHeader};
use crate::{log_and_500, ControlStateDelegate, ControlStateReadAccess, ControlStateWriteAccess, NodeDelegate};

#[derive(Deserialize)]
pub struct CreateIdentityQueryParams {
    email: Option<email_address::EmailAddress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIdentityResponse {
    identity: Identity,
    token: String,
}

pub async fn create_identity<S: ControlStateDelegate + NodeDelegate>(
    State(ctx): State<S>,
    Query(CreateIdentityQueryParams { email }): Query<CreateIdentityQueryParams>,
) -> axum::response::Result<impl IntoResponse> {
    let auth = SpacetimeAuth::alloc(&ctx).await?;
    if let Some(email) = email {
        ctx.add_email(&auth.identity, email.as_str())
            .await
            .map_err(log_and_500)?;
    }

    let identity_response = CreateIdentityResponse {
        identity: auth.identity,
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
    identity: Identity,
    email: String,
}

#[derive(Deserialize)]
pub struct GetIdentityQueryParams {
    email: Option<String>,
}
pub async fn get_identity<S: ControlStateDelegate>(
    State(ctx): State<S>,
    Query(GetIdentityQueryParams { email }): Query<GetIdentityQueryParams>,
) -> axum::response::Result<impl IntoResponse> {
    let lookup = match email {
        None => None,
        Some(email) => {
            let identities = ctx.get_identities_for_email(email.as_str()).map_err(log_and_500)?;
            if identities.is_empty() {
                None
            } else {
                let mut response = GetIdentityResponse {
                    identities: Vec::<GetIdentityResponseEntry>::new(),
                };

                for identity_email in identities {
                    response.identities.push(GetIdentityResponseEntry {
                        identity: identity_email.identity,
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
pub struct SetEmailParams {
    identity: IdentityForUrl,
}

#[derive(Deserialize)]
pub struct SetEmailQueryParams {
    email: email_address::EmailAddress,
}

pub async fn set_email<S: ControlStateWriteAccess>(
    State(ctx): State<S>,
    Path(SetEmailParams { identity }): Path<SetEmailParams>,
    Query(SetEmailQueryParams { email }): Query<SetEmailQueryParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    let identity = identity.into();
    let auth = auth.get().ok_or(StatusCode::BAD_REQUEST)?;

    if auth.identity != identity {
        return Err(StatusCode::UNAUTHORIZED.into());
    }
    ctx.add_email(&identity, email.as_str()).await.map_err(log_and_500)?;

    Ok(())
}

pub async fn check_email<S: ControlStateReadAccess>(
    State(ctx): State<S>,
    Path(SetEmailParams { identity }): Path<SetEmailParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    let identity = identity.into();
    let auth = auth.get().ok_or(StatusCode::BAD_REQUEST)?;

    if auth.identity != identity {
        return Err(StatusCode::UNAUTHORIZED.into());
    }

    let emails = ctx
        .get_emails_for_identity(&identity)
        .map_err(log_and_500)?
        .into_iter()
        .map(|IdentityEmail { email, .. }| email)
        .collect::<Vec<_>>();

    Ok(axum::Json(emails))
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
        .filter(|db| db.identity == identity)
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

#[derive(Deserialize)]
pub struct ValidateTokenParams {
    identity: IdentityForUrl,
}

pub async fn validate_token(
    Path(ValidateTokenParams { identity }): Path<ValidateTokenParams>,
    auth: SpacetimeAuthHeader,
) -> axum::response::Result<impl IntoResponse> {
    let identity = Identity::from(identity);
    if let Some(auth) = auth.auth {
        if auth.identity == identity {
            Ok(StatusCode::NO_CONTENT)
        } else {
            Err(StatusCode::BAD_REQUEST.into())
        }
    } else {
        Err(StatusCode::UNAUTHORIZED.into())
    }
}

pub async fn get_public_key<S: NodeDelegate>(State(ctx): State<S>) -> axum::response::Result<impl IntoResponse> {
    let res = Response::builder()
        .header("Content-Type", "application/pem-certificate-chain")
        .body(())
        .map_err(log_and_500)?;
    Ok((res, ctx.public_key_bytes().to_owned()))
}

pub fn router<S>() -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/", get(get_identity::<S>).post(create_identity::<S>))
        .route("/public-key", get(get_public_key::<S>))
        .route("/websocket_token", post(create_websocket_token::<S>))
        .route("/:identity/verify", get(validate_token))
        .route("/:identity/set-email", post(set_email::<S>))
        .route("/:identity/emails", get(check_email::<S>))
        .route("/:identity/databases", get(get_databases::<S>))
}
