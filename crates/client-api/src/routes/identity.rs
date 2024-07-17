use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Extension;
use chrono::Utc;
use http::header::CONTENT_TYPE;
use http::StatusCode;
use rand::Rng;
use serde::{Deserialize, Serialize};

use spacetimedb::auth::identity::{encode_token, encode_token_with_expiry};
use spacetimedb::messages::control_db::IdentityEmail;
use spacetimedb_client_api_messages::recovery::{RecoveryCode, RecoveryCodeResponse};
use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::{Address, Identity};

use crate::auth::{anon_auth_middleware, SpacetimeAuth, SpacetimeAuthRequired};
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
    match email {
        None => Err(StatusCode::BAD_REQUEST.into()),
        Some(email) => {
            let identities = ctx.get_identities_for_email(email.as_str()).map_err(log_and_500)?;
            let identities = identities
                .into_iter()
                .map(|identity_email| GetIdentityResponseEntry {
                    identity: identity_email.identity,
                    email: identity_email.email,
                })
                .collect::<Vec<_>>();
            Ok(axum::Json(GetIdentityResponse { identities }))
        }
    }
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
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse> {
    let identity = identity.into();

    if auth.identity != identity {
        return Err(StatusCode::UNAUTHORIZED.into());
    }
    ctx.add_email(&identity, email.as_str()).await.map_err(log_and_500)?;

    Ok(())
}

pub async fn check_email<S: ControlStateReadAccess>(
    State(ctx): State<S>,
    Path(SetEmailParams { identity }): Path<SetEmailParams>,
    Extension(auth): Extension<SpacetimeAuth>,
) -> axum::response::Result<impl IntoResponse> {
    let identity = identity.into();

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

#[derive(Deserialize)]
pub struct RequestRecoveryCodeParams {
    /// Whether or not the client is requesting a login link for a web-login. This is false for CLI logins.
    #[serde(default)]
    link: bool,
    email: String,
    identity: IdentityForUrl,
}

pub async fn request_recovery_code<S: NodeDelegate + ControlStateDelegate>(
    State(ctx): State<S>,
    Query(RequestRecoveryCodeParams { link, email, identity }): Query<RequestRecoveryCodeParams>,
) -> axum::response::Result<impl IntoResponse> {
    let identity = Identity::from(identity);
    let Some(sendgrid) = ctx.sendgrid_controller() else {
        log::error!("A recovery code was requested, but SendGrid is disabled.");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "SendGrid is disabled.").into());
    };

    if !ctx
        .get_identities_for_email(email.as_str())
        .map_err(log_and_500)?
        .iter()
        .any(|a| a.identity == identity)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Email is not associated with the provided identity.",
        )
            .into());
    }

    let code = rand::thread_rng().gen_range(0..=999999);
    let code = format!("{code:06}");
    let recovery_code = RecoveryCode {
        code: code.clone(),
        generation_time: Utc::now(),
        identity,
    };
    ctx.insert_recovery_code(&identity, email.as_str(), recovery_code)
        .await
        .map_err(log_and_500)?;

    sendgrid
        .send_recovery_email(email.as_str(), code.as_str(), &identity.to_hex(), link)
        .await
        .map_err(log_and_500)?;
    Ok(())
}

#[derive(Deserialize)]
pub struct ConfirmRecoveryCodeParams {
    pub email: String,
    pub identity: IdentityForUrl,
    pub code: String,
}

/// Note: We should be slightly more security conscious about this function because
///  we are providing a login token to the user initiating the request. We want to make
///  sure there aren't any logical issues in here that would allow a user to request a token
///  for an identity that they don't have authority over.
pub async fn confirm_recovery_code<S: ControlStateDelegate + NodeDelegate>(
    State(ctx): State<S>,
    Query(ConfirmRecoveryCodeParams { email, identity, code }): Query<ConfirmRecoveryCodeParams>,
) -> axum::response::Result<impl IntoResponse> {
    let identity = Identity::from(identity);
    let recovery_codes = ctx.get_recovery_codes(email.as_str()).map_err(log_and_500)?;

    let recovery_code = recovery_codes
        .into_iter()
        .find(|rc| rc.code == code.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "Recovery code not found."))?;

    let duration = Utc::now() - recovery_code.generation_time;
    if duration.num_seconds() > 60 * 10 {
        return Err((StatusCode::BAD_REQUEST, "Recovery code expired.").into());
    }

    // Make sure the identity provided by the request matches the recovery code registration
    if recovery_code.identity != identity {
        return Err((
            StatusCode::BAD_REQUEST,
            "Recovery code doesn't match the provided identity.",
        )
            .into());
    }

    if !ctx
        .get_identities_for_email(email.as_str())
        .map_err(log_and_500)?
        .iter()
        .any(|a| a.identity == identity)
    {
        // This can happen if someone changes their associated email during a recovery request.
        return Err((StatusCode::BAD_REQUEST, "No identity associated with that email.").into());
    }

    // Recovery code is verified, return the identity and token to the user
    let token = encode_token(ctx.private_key(), identity).map_err(log_and_500)?;
    let result = RecoveryCodeResponse { identity, token };

    Ok(axum::Json(result))
}

pub fn router<S>(ctx: S) -> axum::Router<S>
where
    S: NodeDelegate + ControlStateDelegate + Clone + 'static,
{
    use axum::routing::{get, post};
    let auth_middleware = axum::middleware::from_fn_with_state(ctx, anon_auth_middleware::<S>);
    axum::Router::new()
        .route("/", get(get_identity::<S>).post(create_identity::<S>))
        .route("/public-key", get(get_public_key::<S>))
        .route("/request_recovery_code", post(request_recovery_code::<S>))
        .route("/confirm_recovery_code", post(confirm_recovery_code::<S>))
        .route("/websocket_token", post(create_websocket_token::<S>))
        .route("/:identity/verify", get(validate_token))
        .route(
            "/:identity/set-email",
            post(set_email::<S>).route_layer(auth_middleware.clone()),
        )
        .route("/:identity/emails", get(check_email::<S>).route_layer(auth_middleware))
        .route("/:identity/databases", get(get_databases::<S>))
}
