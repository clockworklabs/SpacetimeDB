use std::time::{Duration, SystemTime};

use axum::extract::{Query, Request, State};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum_extra::typed_header::TypedHeader;
use headers::{authorization, HeaderMapExt};
use http::{request, HeaderValue, StatusCode};
use serde::{Deserialize, Serialize};
use spacetimedb::auth::identity::SpacetimeIdentityClaims;
use spacetimedb::auth::identity::{JwtError, JwtErrorKind};
use spacetimedb::auth::token_validation::{
    new_validator, DefaultValidator, TokenSigner, TokenValidationError, TokenValidator,
};
use spacetimedb::auth::JwtKeys;
use spacetimedb::energy::EnergyQuanta;
use spacetimedb::identity::Identity;
use uuid::Uuid;

use crate::{log_and_500, ControlStateDelegate, NodeDelegate};

/// Credentials for login for a spacetime identity, represented as a JWT.
///
/// This can be passed as a header `Authentication: Bearer $token` or as
/// a query param `?token=$token`, with the former taking precedence over
/// the latter.
#[derive(Clone, Deserialize)]
pub struct SpacetimeCreds {
    token: String,
}

pub const LOCALHOST: &str = "localhost";

impl SpacetimeCreds {
    /// The JWT token representing these credentials.
    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn from_signed_token(token: String) -> Self {
        Self { token }
    }

    pub fn to_header_value(&self) -> HeaderValue {
        let mut val = HeaderValue::try_from(["Bearer ", self.token()].concat()).unwrap();
        val.set_sensitive(true);
        val
    }

    /// Extract credentials from the headers or else query string of a request.
    fn from_request_parts(parts: &request::Parts) -> Result<Option<Self>, headers::Error> {
        let header = parts
            .headers
            .typed_try_get::<headers::Authorization<authorization::Bearer>>()?;
        if let Some(headers::Authorization(bearer)) = header {
            let token = bearer.token().to_owned();
            return Ok(Some(SpacetimeCreds { token }));
        }
        if let Ok(Query(creds)) = Query::<Self>::try_from_uri(&parts.uri) {
            return Ok(Some(creds));
        }
        Ok(None)
    }
}

/// The auth information in a request.
///
/// This is inserted as an extension by [`auth_middleware`]; make sure that's applied if you're making expecting
/// this to be present.
#[derive(Clone)]
pub struct SpacetimeAuth {
    pub creds: SpacetimeCreds,
    pub identity: Identity,
    pub subject: String,
    pub issuer: String,
}

use jsonwebtoken;

pub struct TokenClaims {
    pub issuer: String,
    pub subject: String,
    pub audience: Vec<String>,
}

impl From<SpacetimeAuth> for TokenClaims {
    fn from(claims: SpacetimeAuth) -> Self {
        Self {
            issuer: claims.issuer,
            subject: claims.subject,
            // This will need to be changed when we care about audiencies.
            audience: Vec::new(),
        }
    }
}

impl TokenClaims {
    pub fn new(issuer: String, subject: String) -> Self {
        Self {
            issuer,
            subject,
            audience: Vec::new(),
        }
    }

    // Compute the id from the issuer and subject.
    pub fn id(&self) -> Identity {
        Identity::from_claims(&self.issuer, &self.subject)
    }

    pub fn encode_and_sign_with_expiry(
        &self,
        signer: &impl TokenSigner,
        expiry: Option<Duration>,
    ) -> Result<String, JwtError> {
        let iat = SystemTime::now();
        let exp = expiry.map(|dur| iat + dur);
        let claims = SpacetimeIdentityClaims {
            identity: self.id(),
            subject: self.subject.clone(),
            issuer: self.issuer.clone(),
            audience: self.audience.clone(),
            iat,
            exp,
        };
        signer.sign(&claims)
    }

    pub fn encode_and_sign(&self, signer: &impl TokenSigner) -> Result<String, JwtError> {
        self.encode_and_sign_with_expiry(signer, None)
    }
}

impl SpacetimeAuth {
    pub fn from_claims(
        ctx: &(impl NodeDelegate + ControlStateDelegate + ?Sized),
        claims: SpacetimeIdentityClaims,
    ) -> axum::response::Result<Self> {
        let claims = TokenClaims {
            issuer: claims.issuer,
            subject: claims.subject,
            audience: claims.audience,
        };

        let creds = {
            let token = claims.encode_and_sign(ctx.jwt_auth_provider()).map_err(log_and_500)?;
            SpacetimeCreds::from_signed_token(token)
        };
        let identity = claims.id();

        Ok(Self {
            creds,
            identity,
            subject: claims.subject,
            issuer: claims.issuer,
        })
    }

    /// Allocate a new identity, and mint a new token for it.
    pub async fn alloc(ctx: &(impl NodeDelegate + ControlStateDelegate + ?Sized)) -> axum::response::Result<Self> {
        // Generate claims with a random subject.
        let subject = Uuid::new_v4().to_string();
        let claims = TokenClaims {
            issuer: ctx.jwt_auth_provider().local_issuer().to_owned(),
            subject: subject.clone(),
            // Placeholder audience.
            audience: vec!["spacetimedb".to_string()],
        };

        let identity = claims.id();
        let creds = {
            let token = claims.encode_and_sign(ctx.jwt_auth_provider()).map_err(log_and_500)?;
            SpacetimeCreds::from_signed_token(token)
        };

        Ok(Self {
            creds,
            identity,
            subject,
            issuer: ctx.jwt_auth_provider().local_issuer().to_string(),
        })
    }

    /// Get the auth credentials as headers to be returned from an endpoint.
    pub fn into_headers(self) -> (TypedHeader<SpacetimeIdentity>, TypedHeader<SpacetimeIdentityToken>) {
        (
            TypedHeader(SpacetimeIdentity(self.identity)),
            TypedHeader(SpacetimeIdentityToken(self.creds)),
        )
    }

    // Sign a new token with the same claims and a new expiry.
    // Note that this will not change the issuer, so the private_key might not match.
    // We do this to create short-lived tokens that we will be able to verify.
    pub fn re_sign_with_expiry(&self, signer: &impl TokenSigner, expiry: Duration) -> Result<String, JwtError> {
        TokenClaims::from(self.clone()).encode_and_sign_with_expiry(signer, Some(expiry))
    }
}

// JwtAuthProvider is used for signing and verifying JWT tokens.
pub trait JwtAuthProvider: Sync + Send + TokenSigner {
    type TV: TokenValidator + Send + Sync;
    /// Used to validate incoming JWTs.
    fn validator(&self) -> &Self::TV;

    /// The issuer to use when signing JWTs.
    fn local_issuer(&self) -> &str;

    /// Return the public key used to verify JWTs, as the bytes of a PEM public key file.
    ///
    /// The `/identity/public-key` route calls this method to return the public key to callers.
    fn public_key_bytes(&self) -> &[u8];
    /// Return the private key used to verify JWTs, as the bytes of a PEM private key file.
    fn private_key_bytes(&self) -> &[u8];
}

pub struct JwtKeyAuthProvider<TV: TokenValidator + Send + Sync> {
    keys: JwtKeys,
    local_issuer: String,
    validator: TV,
}

pub type DefaultJwtAuthProvider = JwtKeyAuthProvider<DefaultValidator>;

// Create a new AuthEnvironment using the default caching validator.
pub fn default_auth_environment(keys: JwtKeys, local_issuer: String) -> JwtKeyAuthProvider<DefaultValidator> {
    let validator = new_validator(keys.public.clone(), local_issuer.clone());
    JwtKeyAuthProvider::new(keys, local_issuer, validator)
}

impl<TV: TokenValidator + Send + Sync> JwtKeyAuthProvider<TV> {
    fn new(keys: JwtKeys, local_issuer: String, validator: TV) -> Self {
        Self {
            keys,
            local_issuer,
            validator,
        }
    }
}

impl<TV: TokenValidator + Send + Sync> TokenSigner for JwtKeyAuthProvider<TV> {
    fn sign<T: Serialize>(&self, claims: &T) -> Result<String, JwtError> {
        let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        jsonwebtoken::encode(&header, &claims, &self.keys.private)
    }
}

impl<TV: TokenValidator + Send + Sync> JwtAuthProvider for JwtKeyAuthProvider<TV> {
    type TV = TV;

    fn validator(&self) -> &Self::TV {
        &self.validator
    }

    fn local_issuer(&self) -> &str {
        &self.local_issuer
    }

    fn public_key_bytes(&self) -> &[u8] {
        &self.keys.public_pem
    }

    fn private_key_bytes(&self) -> &[u8] {
        &self.keys.private_pem
    }
}

#[cfg(test)]
mod tests {
    use crate::auth::TokenClaims;
    use anyhow::Ok;
    use spacetimedb::auth::{token_validation::TokenValidator, JwtKeys};

    // Make sure that when we encode TokenClaims, we can decode to get the expected identity.
    #[tokio::test]
    async fn decode_encoded_token() -> Result<(), anyhow::Error> {
        let kp = JwtKeys::generate()?;

        let claims = TokenClaims {
            issuer: "localhost".to_string(),
            subject: "test-subject".to_string(),
            audience: vec!["spacetimedb".to_string()],
        };
        let id = claims.id();
        let token = claims.encode_and_sign(&kp.private)?;
        let decoded = kp.public.validate_token(&token).await?;

        assert_eq!(decoded.identity, id);
        Ok(())
    }
}

pub async fn validate_token<S: NodeDelegate>(
    state: &S,
    token: &str,
) -> Result<SpacetimeIdentityClaims, TokenValidationError> {
    state.jwt_auth_provider().validator().validate_token(token).await
}

pub struct SpacetimeAuthHeader {
    auth: Option<SpacetimeAuth>,
}

#[async_trait::async_trait]
impl<S: NodeDelegate + Send + Sync> axum::extract::FromRequestParts<S> for SpacetimeAuthHeader {
    type Rejection = AuthorizationRejection;
    async fn from_request_parts(parts: &mut request::Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Some(creds) = SpacetimeCreds::from_request_parts(parts)? else {
            return Ok(Self { auth: None });
        };

        let claims = validate_token(state, &creds.token)
            .await
            .map_err(AuthorizationRejection::Custom)?;

        let auth = SpacetimeAuth {
            creds,
            identity: claims.identity,
            subject: claims.subject,
            issuer: claims.issuer,
        };
        Ok(Self { auth: Some(auth) })
    }
}

/// A response by the API signifying that an authorization was rejected with the `reason` for this.
#[derive(Debug, derive_more::From)]
pub enum AuthorizationRejection {
    Jwt(JwtError),
    Header(headers::Error),
    Custom(TokenValidationError),
    Required,
}

impl IntoResponse for AuthorizationRejection {
    fn into_response(self) -> axum::response::Response {
        // Most likely, the server key was rotated.
        const ROTATED: (StatusCode, &str) = (
            StatusCode::UNAUTHORIZED,
            "Authorization failed: token not signed by this instance",
        );
        // The JWT is malformed, see SpacetimeCreds for specifics on the format.
        const INVALID: (StatusCode, &str) = (StatusCode::BAD_REQUEST, "Authorization is invalid: malformed token");
        // Sensible fallback if no auth header is present.
        const REQUIRED: (StatusCode, &str) = (StatusCode::UNAUTHORIZED, "Authorization required");

        log::trace!("Authorization rejection: {:?}", self);

        match self {
            AuthorizationRejection::Jwt(e) if *e.kind() == JwtErrorKind::InvalidSignature => ROTATED.into_response(),
            AuthorizationRejection::Jwt(_) | AuthorizationRejection::Header(_) => INVALID.into_response(),
            AuthorizationRejection::Custom(msg) => (StatusCode::UNAUTHORIZED, format!("{:?}", msg)).into_response(),
            AuthorizationRejection::Required => REQUIRED.into_response(),
        }
    }
}

impl SpacetimeAuthHeader {
    pub fn get(self) -> Option<SpacetimeAuth> {
        self.auth
    }

    /// Given an authorization header we will try to get the identity and token from the auth header (as JWT).
    /// If there is no JWT in the auth header we will create a new identity and token and return it.
    pub async fn get_or_create(
        self,
        ctx: &(impl NodeDelegate + ControlStateDelegate + ?Sized),
    ) -> axum::response::Result<SpacetimeAuth> {
        match self.auth {
            Some(auth) => Ok(auth),
            None => SpacetimeAuth::alloc(ctx).await,
        }
    }
}

pub struct SpacetimeAuthRequired(pub SpacetimeAuth);

#[async_trait::async_trait]
impl<S: NodeDelegate + Send + Sync> axum::extract::FromRequestParts<S> for SpacetimeAuthRequired {
    type Rejection = AuthorizationRejection;
    async fn from_request_parts(parts: &mut request::Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth = SpacetimeAuthHeader::from_request_parts(parts, state).await?;
        let auth = auth.get().ok_or(AuthorizationRejection::Required)?;
        Ok(SpacetimeAuthRequired(auth))
    }
}

pub struct SpacetimeIdentity(pub Identity);
impl headers::Header for SpacetimeIdentity {
    fn name() -> &'static http::HeaderName {
        static NAME: http::HeaderName = http::HeaderName::from_static("spacetime-identity");
        &NAME
    }

    fn decode<'i, I: Iterator<Item = &'i HeaderValue>>(_values: &mut I) -> Result<Self, headers::Error> {
        unimplemented!()
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend([self.0.to_hex().as_str().try_into().unwrap()])
    }
}

pub struct SpacetimeIdentityToken(pub SpacetimeCreds);
impl headers::Header for SpacetimeIdentityToken {
    fn name() -> &'static http::HeaderName {
        static NAME: http::HeaderName = http::HeaderName::from_static("spacetime-identity-token");
        &NAME
    }

    fn decode<'i, I: Iterator<Item = &'i HeaderValue>>(_values: &mut I) -> Result<Self, headers::Error> {
        unimplemented!()
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend([self.0.token().try_into().unwrap()])
    }
}

pub struct SpacetimeEnergyUsed(pub EnergyQuanta);
impl headers::Header for SpacetimeEnergyUsed {
    fn name() -> &'static http::HeaderName {
        static NAME: http::HeaderName = http::HeaderName::from_static("spacetime-energy-used");
        &NAME
    }

    fn decode<'i, I: Iterator<Item = &'i HeaderValue>>(_values: &mut I) -> Result<Self, headers::Error> {
        unimplemented!()
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        let mut buf = itoa::Buffer::new();
        let value = buf.format(self.0.get());
        values.extend([value.try_into().unwrap()]);
    }
}

pub struct SpacetimeExecutionDurationMicros(pub Duration);
impl headers::Header for SpacetimeExecutionDurationMicros {
    fn name() -> &'static http::HeaderName {
        static NAME: http::HeaderName = http::HeaderName::from_static("spacetime-execution-duration-micros");
        &NAME
    }

    fn decode<'i, I: Iterator<Item = &'i HeaderValue>>(_values: &mut I) -> Result<Self, headers::Error> {
        unimplemented!()
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend([(self.0.as_micros() as u64).into()])
    }
}

pub async fn anon_auth_middleware<S: ControlStateDelegate + NodeDelegate>(
    State(worker_ctx): State<S>,
    auth: SpacetimeAuthHeader,
    mut req: Request,
    next: Next,
) -> axum::response::Result<impl IntoResponse> {
    let auth = auth.get_or_create(&worker_ctx).await?;
    req.extensions_mut().insert(auth.clone());
    let resp = next.run(req).await;
    Ok((auth.into_headers(), resp))
}
