use anyhow;
use async_cache;
use async_trait::async_trait;
use core::ops::Deref;
use faststr::FastStr;
use jsonwebtoken::decode_header;
pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::errors::ErrorKind as JwtErrorKind;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, AlgorithmFamily, Validation};
pub use jsonwebtoken::{DecodingKey, EncodingKey};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror;

use super::identity::{IncomingClaims, SpacetimeIdentityClaims};
use super::JwtKeys;

#[derive(thiserror::Error, Debug)]
pub enum TokenValidationError {
    // TODO: Add real error types.

    // TODO: If we had our own errors defined we wouldn't be locked into this lib.
    #[error("Invalid token: {0}")]
    TokenError(#[from] JwtError),

    #[error("Specified key ID not found in JWKs")]
    KeyIDNotFound,

    #[error("OIDC/JWKS request failed: {0}")]
    OidcRequestError(#[from] reqwest::Error),

    // The other case is a catch-all for unexpected errors.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

// A token signer is responsible for signing tokens without doing any validation.
pub trait TokenSigner: Sync + Send {
    // Serialize the given claims and sign a JWT token with them as the payload.
    fn sign<T: Serialize>(&self, claims: &T) -> Result<String, JwtError>;
}

impl TokenSigner for EncodingKey {
    fn sign<Token: Serialize>(&self, claims: &Token) -> Result<String, JwtError> {
        let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        jsonwebtoken::encode(&header, claims, self)
    }
}

impl TokenSigner for JwtKeys {
    fn sign<Token: Serialize>(&self, claims: &Token) -> Result<String, JwtError> {
        self.private.sign(claims)
    }
}

// A TokenValidator is responsible for validating a token and returning the claims.
// This includes looking up the public key for the issuer and verifying the signature.
// It is also responsible for enforcing the rules around the claims.
// For example, this must ensure that the issuer and sub are no longer than 128 bytes.
#[async_trait]
pub trait TokenValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims, TokenValidationError>;
}

#[async_trait]
impl<T: TokenValidator + Send + Sync> TokenValidator for Arc<T> {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims, TokenValidationError> {
        (**self).validate_token(token).await
    }
}

pub struct UnimplementedTokenValidator;

#[async_trait]
impl TokenValidator for UnimplementedTokenValidator {
    async fn validate_token(&self, _token: &str) -> Result<SpacetimeIdentityClaims, TokenValidationError> {
        Err(TokenValidationError::Other(anyhow::anyhow!("Unimplemented")))
    }
}

// This validator accepts any tokens signed with the local key (regardless of issuer).
// If it is not signed with the local key, we will try to validate it with the OIDC validator.
// We do this because we sign short lived tokens with different issuers.
pub struct FullTokenValidator<T: TokenValidator + Send + Sync> {
    pub local_key: DecodingKey,
    pub local_issuer: Box<str>,
    pub oidc_validator: T,
}

#[async_trait]
impl<T> TokenValidator for FullTokenValidator<T>
where
    T: TokenValidator + Send + Sync,
{
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims, TokenValidationError> {
        let local_key_error = {
            let first_validator = BasicTokenValidator {
                public_key: self.local_key.clone(),
                issuer: None,
            };
            match first_validator.validate_token(token).await {
                Ok(claims) => return Ok(claims),
                Err(e) => e,
            }
        };

        // If that fails, we try the OIDC validator.
        let issuer = get_raw_issuer(token)?;
        // If we are the issuer, then we should have already validated the token.
        // TODO: "localhost" should not be hard-coded.
        if issuer == self.local_issuer {
            return Err(local_key_error);
        }
        self.oidc_validator.validate_token(token).await
    }
}

pub type DefaultValidator = FullTokenValidator<CachingOidcTokenValidator>;

pub fn new_validator(local_key: DecodingKey, local_issuer: Box<str>) -> FullTokenValidator<CachingOidcTokenValidator> {
    FullTokenValidator {
        local_key,
        local_issuer,
        oidc_validator: CachingOidcTokenValidator::get_default(),
    }
}

// This verifies against a given public key and expected issuer.
// The issuer should only be None if we are checking with a local key.
// We do that because we signed short-lived keys with different issuers.
struct BasicTokenValidator {
    pub public_key: DecodingKey,
    pub issuer: Option<Box<str>>,
}

lazy_static! {
    // Eventually we will want to add more required claims.
    static ref REQUIRED_CLAIMS: Vec<&'static str> = vec!["sub", "iss"];
}

#[async_trait]
impl TokenValidator for DecodingKey {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims, TokenValidationError> {
        let mut validation = Validation::new(jsonwebtoken::Algorithm::ES256);
        validation.algorithms = match self.family() {
            AlgorithmFamily::Ec => vec![jsonwebtoken::Algorithm::ES256],
            AlgorithmFamily::Rsa => vec![jsonwebtoken::Algorithm::RS256],
            AlgorithmFamily::Hmac => vec![jsonwebtoken::Algorithm::HS256],
            AlgorithmFamily::Ed => {
                // Preserve the pre-upgrade policy: SpacetimeDB only accepted ES256, RS256, and HS256 here.
                return Err(TokenValidationError::TokenError(JwtErrorKind::InvalidAlgorithm.into()));
            }
        };
        validation.set_required_spec_claims(&REQUIRED_CLAIMS);

        // TODO: We should require a specific audience at some point.
        validation.validate_aud = false;

        // Note, `jsonwebtoken` rejects `"exp": null` before deserializing claims.
        // However, older SpacetimeDB tokens used `"exp": null` to encoded no expiration.
        // Those tokens may still be cached by clients, so we verify the signature with the crate
        // but preserve our historical `None` means no expiry semantics below.
        validation.validate_exp = false;

        let data = decode::<IncomingClaims>(token, self, &validation)?;
        let claims = data.claims;
        validate_expiration(&claims)?;
        claims.try_into().map_err(TokenValidationError::Other)
    }
}

fn validate_expiration(claims: &IncomingClaims) -> Result<(), JwtError> {
    if let Some(exp) = claims.exp {
        // Match jsonwebtoken's default 60s leeway while allowing `None`/`null` to mean no expiration.
        if SystemTime::now()
            .duration_since(exp)
            .is_ok_and(|elapsed| elapsed > Duration::from_secs(60))
        {
            return Err(JwtErrorKind::ExpiredSignature.into());
        }
    }
    Ok(())
}

#[async_trait]
impl TokenValidator for BasicTokenValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims, TokenValidationError> {
        // This validates everything but the issuer.
        let claims = self.public_key.validate_token(token).await?;
        if let Some(expected_issuer) = &self.issuer
            && *claims.issuer != **expected_issuer
        {
            return Err(TokenValidationError::Other(anyhow::anyhow!(
                "Issuer mismatch: got {:?}, expected {:?}",
                claims.issuer,
                expected_issuer
            )));
        }
        Ok(claims)
    }
}

// Validates tokens by looking up public keys and caching them.
pub struct CachingOidcTokenValidator {
    cache: async_cache::AsyncCache<Arc<JwksValidator>, KeyFetcher>,
}

impl CachingOidcTokenValidator {
    pub fn new(refresh_duration: Duration, expiry: Option<Duration>) -> Self {
        let cache = async_cache::Options::new(refresh_duration, KeyFetcher)
            .with_expire(expiry)
            .build();
        CachingOidcTokenValidator { cache }
    }

    pub fn get_default() -> Self {
        Self::new(Duration::from_secs(300), Some(Duration::from_secs(7200)))
    }
}

// Jwks fetcher for the async cache.
struct KeyFetcher;

impl async_cache::Fetcher<Arc<JwksValidator>> for KeyFetcher {
    type Error = TokenValidationError;

    async fn fetch(&self, key: FastStr) -> Result<Arc<JwksValidator>, Self::Error> {
        // TODO: Make this stored in the struct so we don't need to keep creating it.
        let raw_issuer = key.deref();
        log::info!("Fetching key for issuer {}", raw_issuer);
        let oidc_url = format!("{}/.well-known/openid-configuration", raw_issuer.trim_end_matches('/'));
        let key_or_error = JsonWebKeySet::from_oidc_url(&oidc_url).await;
        // TODO: We should probably add debouncing to avoid spamming the logs.
        // Alternatively we could add a backoff before retrying.
        if let Err(e) = &key_or_error {
            log::warn!("Error fetching public key for issuer {raw_issuer}: {e:?}");
        }
        let keys = key_or_error?;
        let validator = JwksValidator {
            issuer: raw_issuer.into(),
            keyset: keys,
        };
        Ok(Arc::new(validator))
    }
}

#[async_trait]
impl TokenValidator for CachingOidcTokenValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims, TokenValidationError> {
        let raw_issuer = get_raw_issuer(token)?;
        log::debug!("Getting validator for issuer {}", raw_issuer.clone());
        let validator = self
            .cache
            .get(String::from(raw_issuer.clone()).into())
            .await
            .ok_or_else(|| anyhow::anyhow!("Error fetching public key for issuer {raw_issuer}"))?;
        validator.validate_token(token).await
    }
}

// This is a token validator that uses OIDC to validate tokens.
// This will look up the public key for the issuer and validate against that key.
// This currently has no caching.
pub struct OidcTokenValidator;

// Get the issuer out of a token without validating the signature.
fn get_raw_issuer(token: &str) -> Result<Box<str>, TokenValidationError> {
    // We need the issuer before we know which key to use.
    // This intentionally does not validate the token.
    // Callers must only use it for key discovery and must verify the token afterwards.
    let data = jsonwebtoken::dangerous::insecure_decode::<IncomingClaims>(token)?;
    Ok(data.claims.issuer)
}

#[async_trait]
impl TokenValidator for OidcTokenValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims, TokenValidationError> {
        // TODO: Make this stored in the struct so we don't need to keep creating it.
        let raw_issuer = get_raw_issuer(token)?;
        let oidc_url = format!("{}/.well-known/openid-configuration", raw_issuer.trim_end_matches('/'));
        log::debug!("Fetching key for issuer {}", raw_issuer.clone());
        let key_or_error = JsonWebKeySet::from_oidc_url(&oidc_url).await;
        // TODO: We should probably add debouncing to avoid spamming the logs.
        // Alternatively we could add a backoff before retrying.
        if let Err(e) = &key_or_error {
            log::warn!("Error fetching public key for issuer {raw_issuer}: {e:?}");
        }
        let keys = key_or_error?;
        let validator = JwksValidator {
            issuer: raw_issuer,
            keyset: keys,
        };
        validator.validate_token(token).await
    }
}

struct JwksValidator {
    pub issuer: Box<str>,
    pub keyset: JsonWebKeySet,
}

#[async_trait]
impl TokenValidator for JwksValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims, TokenValidationError> {
        let header = decode_header(token)?;
        if let Some(kid) = header.kid.as_deref() {
            if let Some(key) = self.keyset.key_with_id(kid) {
                return self.validate_with_key(token, key).await;
            }

            log::debug!("Key id {kid} not found in JWKS. Trying keys without key ids.");
            let mut last_error = TokenValidationError::KeyIDNotFound;
            for key in self.keyset.keys_without_ids() {
                match self.validate_with_key(token, key).await {
                    Ok(claims) => return Ok(claims),
                    Err(e) => {
                        last_error = e;
                        log::debug!("Validating with key without kid failed");
                    }
                }
            }
            return Err(last_error);
        }
        log::debug!("No key id in header. Trying all keys.");
        // TODO: Consider returning an error if no kid is given?
        // For now, lets just try all the keys.
        let mut last_error = TokenValidationError::Other(anyhow::anyhow!("No kid found"));
        for key in &self.keyset.keys {
            match &key.kid {
                Some(kid) => log::debug!("Trying key {kid}"),
                None => log::debug!("Trying key without kid"),
            }
            match self.validate_with_key(token, key).await {
                Ok(claims) => return Ok(claims),
                Err(e) => {
                    last_error = e;
                    log::debug!("Validating with JWKS key failed");
                    continue;
                }
            }
        }
        // None of the keys worked.
        Err(last_error)
    }
}

impl JwksValidator {
    async fn validate_with_key(
        &self,
        token: &str,
        key: &JsonWebKey,
    ) -> Result<SpacetimeIdentityClaims, TokenValidationError> {
        let validator = BasicTokenValidator {
            public_key: key.decoding_key.clone(),
            issuer: Some(self.issuer.clone()),
        };
        validator.validate_token(token).await
    }
}

#[derive(Deserialize)]
struct OidcConfig {
    jwks_uri: String,
}

struct JsonWebKeySet {
    keys: Vec<JsonWebKey>,
}

struct JsonWebKey {
    kid: Option<Box<str>>,
    decoding_key: DecodingKey,
}

impl JsonWebKeySet {
    async fn from_oidc_url(oidc_url: &str) -> Result<Self, TokenValidationError> {
        // We used to depend on the `jwks` crate for this small amount of glue code.
        // Keep the fetch path local so the jsonwebtoken 10 upgrade does not force in
        // jwks' reqwest 0.13/rustls dependency tree alongside the workspace reqwest.
        validate_url_scheme(oidc_url)?;
        let client = reqwest::Client::default();
        let oidc_config = client.get(oidc_url).send().await?.json::<OidcConfig>().await?;
        let jwks = client.get(oidc_config.jwks_uri).send().await?.json::<JwkSet>().await?;
        jwks.try_into()
    }
}

impl TryFrom<JwkSet> for JsonWebKeySet {
    type Error = TokenValidationError;

    fn try_from(jwks: JwkSet) -> Result<Self, Self::Error> {
        let mut keys = Vec::with_capacity(jwks.keys.len());
        for jwk in jwks.keys {
            // `kid` is optional in both JWT headers and JWKs.
            // Use it as a fast path when present,
            // but keep JWKs without `kid` so standards-compliant providers work.
            let kid = jwk.common.key_id.clone().map(Into::into);
            let decoding_key = DecodingKey::from_jwk(&jwk)?;
            keys.push(JsonWebKey { kid, decoding_key });
        }
        Ok(Self { keys })
    }
}

impl JsonWebKeySet {
    fn key_with_id(&self, kid: &str) -> Option<&JsonWebKey> {
        self.keys.iter().find(|key| key.kid.as_deref() == Some(kid))
    }

    fn keys_without_ids(&self) -> impl Iterator<Item = &JsonWebKey> {
        self.keys.iter().filter(|key| key.kid.is_none())
    }
}

fn validate_url_scheme(url: &str) -> Result<(), TokenValidationError> {
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(())
    } else {
        Err(TokenValidationError::Other(anyhow::anyhow!(
            "Invalid OIDC URL scheme: {url}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::auth::identity::{IncomingClaims, SpacetimeIdentityClaims};
    use crate::auth::token_validation::{
        BasicTokenValidator, CachingOidcTokenValidator, FullTokenValidator, JwtErrorKind, OidcTokenValidator,
        TokenSigner, TokenValidationError, TokenValidator,
    };
    use crate::auth::JwtKeys;
    use base64::Engine;
    use openssl::ec::{EcGroup, EcKey};
    use serde_json;
    use spacetimedb_lib::Identity;

    #[tokio::test]
    async fn test_local_validator_checks_issuer() -> anyhow::Result<()> {
        // Test that the issuer must match the expected issuer for LocalTokenValidator.
        let kp = JwtKeys::generate()?;
        let issuer = "test1";
        let subject = "test_subject";

        let orig_claims = IncomingClaims {
            identity: None,
            subject: subject.into(),
            issuer: issuer.into(),
            audience: [].into(),
            iat: std::time::SystemTime::now(),
            exp: None,
            extra: None,
        };
        let token = kp.private.sign(&orig_claims)?;

        {
            // Test that we can validate it.
            let validator = BasicTokenValidator {
                public_key: kp.public.clone(),
                issuer: Some(issuer.into()),
            };

            let parsed_claims: SpacetimeIdentityClaims = validator.validate_token(&token).await?;
            assert_eq!(&*parsed_claims.issuer, issuer);
            assert_eq!(&*parsed_claims.subject, subject);
            assert_eq!(parsed_claims.identity, Identity::from_claims(issuer, subject));
        }
        {
            // Now try with the wrong expected issuer.
            let validator = BasicTokenValidator {
                public_key: kp.public.clone(),
                issuer: Some("otherissuer".into()),
            };

            assert!(validator.validate_token(&token).await.is_err());
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_local_validator_checks_key() -> anyhow::Result<()> {
        // Test that the decoding key must work for LocalTokenValidator.
        let kp = JwtKeys::generate()?;
        let issuer = "test1";
        let subject = "test_subject";

        let orig_claims = IncomingClaims {
            identity: None,
            subject: subject.into(),
            issuer: issuer.into(),
            audience: [].into(),
            iat: std::time::SystemTime::now(),
            exp: None,
            extra: None,
        };
        let token = kp.private.sign(&orig_claims)?;

        {
            // Test that we can validate it.
            let validator = BasicTokenValidator {
                public_key: kp.public.clone(),
                issuer: Some(issuer.into()),
            };

            let parsed_claims: SpacetimeIdentityClaims = validator.validate_token(&token).await?;
            assert_eq!(&*parsed_claims.issuer, issuer);
            assert_eq!(&*parsed_claims.subject, subject);
            assert_eq!(parsed_claims.identity, Identity::from_claims(issuer, subject));
        }
        {
            // We generate a new keypair and try to decode with that key.
            let other_kp = JwtKeys::generate()?;
            // Now try with the wrong expected issuer.
            let validator = BasicTokenValidator {
                public_key: other_kp.public.clone(),
                issuer: Some("otherissuer".into()),
            };

            assert!(validator.validate_token(&token).await.is_err());
        }

        Ok(())
    }

    #[tokio::test]
    async fn accept_legacy_null_exp_tokens() -> anyhow::Result<()> {
        let kp = JwtKeys::generate()?;
        let issuer = "test1";
        let subject = "test_subject";
        let orig_claims = LegacyIdentityClaims {
            identity: Identity::from_claims(issuer, subject),
            subject: subject.into(),
            issuer: issuer.into(),
            audience: [].into(),
            iat: std::time::SystemTime::now(),
            exp: None,
        };
        let token = kp.private.sign(&orig_claims)?;

        let parsed_claims = kp.public.validate_token(&token).await?;
        assert_eq!(&*parsed_claims.issuer, issuer);
        assert_eq!(&*parsed_claims.subject, subject);
        assert_eq!(parsed_claims.identity, Identity::from_claims(issuer, subject));
        Ok(())
    }

    #[tokio::test]
    async fn reject_explicit_expired_tokens() -> anyhow::Result<()> {
        let kp = JwtKeys::generate()?;
        let issuer = "test1";
        let subject = "test_subject";
        let orig_claims = LegacyIdentityClaims {
            identity: Identity::from_claims(issuer, subject),
            subject: subject.into(),
            issuer: issuer.into(),
            audience: [].into(),
            iat: std::time::SystemTime::now(),
            exp: Some(std::time::SystemTime::now() - Duration::from_secs(120)),
        };
        let token = kp.private.sign(&orig_claims)?;

        let err = kp.public.validate_token(&token).await.unwrap_err();
        match err {
            TokenValidationError::TokenError(err) => {
                assert_eq!(err.kind(), &JwtErrorKind::ExpiredSignature);
            }
            err => anyhow::bail!("expected expired signature, got {err:?}"),
        }
        Ok(())
    }

    async fn assert_validation_fails<T: TokenValidator>(validator: &T, token: &str) -> anyhow::Result<()> {
        let result = validator.validate_token(token).await;
        if let Ok(claims) = result {
            anyhow::bail!("Validation succeeded when it should have failed: {:?}", claims);
        }
        Ok(())
    }

    #[tokio::test]
    async fn resigned_token_ignores_issuer() -> anyhow::Result<()> {
        // Test that the decoding key must work for LocalTokenValidator.
        let kp = JwtKeys::generate()?;
        let local_issuer = "test1";
        let external_issuer = "other_issuer";
        let subject = "test_subject";

        let orig_claims = IncomingClaims {
            identity: None,
            subject: subject.into(),
            issuer: external_issuer.into(),
            audience: [].into(),
            iat: std::time::SystemTime::now(),
            exp: None,
            extra: None,
        };
        let token = kp.private.sign(&orig_claims)?;

        // First, try the successful case with the FullTokenValidator.
        {
            let validator = FullTokenValidator {
                local_key: kp.public.clone(),
                local_issuer: local_issuer.into(),
                oidc_validator: OidcTokenValidator,
            };

            let parsed_claims: SpacetimeIdentityClaims = validator.validate_token(&token).await?;
            assert_eq!(&*parsed_claims.issuer, external_issuer);
            assert_eq!(&*parsed_claims.subject, subject);
            assert_eq!(parsed_claims.identity, Identity::from_claims(external_issuer, subject));
        }
        // Double check that this token would fail with an OidcTokenValidator.
        assert_validation_fails(&OidcTokenValidator, &token).await?;
        // Double check that validation fails if we check the issuer.
        assert_validation_fails(
            &BasicTokenValidator {
                public_key: kp.public.clone(),
                issuer: Some(local_issuer.into()),
            },
            &token,
        )
        .await?;
        Ok(())
    }

    use axum::routing::get;
    use axum::Json;
    use axum::Router;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    use serde::{Deserialize, Serialize};
    #[derive(Deserialize, Serialize, Clone)]
    struct OIDCConfig {
        jwks_uri: String,
    }

    #[serde_with::serde_as]
    #[derive(Serialize)]
    struct LegacyIdentityClaims {
        #[serde(rename = "hex_identity")]
        identity: Identity,
        #[serde(rename = "sub")]
        subject: Box<str>,
        #[serde(rename = "iss")]
        issuer: Box<str>,
        #[serde(rename = "aud")]
        audience: Box<[Box<str>]>,
        #[serde_as(as = "serde_with::TimestampSeconds")]
        iat: std::time::SystemTime,
        // This intentionally lacks `skip_serializing_if`.
        // It models no-expiration tokens minted by the previous JWT fork as `"exp": null`.
        #[serde_as(as = "Option<serde_with::TimestampSeconds>")]
        exp: Option<std::time::SystemTime>,
    }

    async fn oidc_config_handler(config: OIDCConfig) -> Json<OIDCConfig> {
        Json(config)
    }

    // You can drop this to shut down the server.
    // This will host an oidc config at `{base_url}/.well-known/openid-configuration`
    // It will also host jwks at `{base_url}/jwks.json`
    struct OIDCServerHandle {
        pub base_url: String,
        #[allow(dead_code)]
        pub shutdown_tx: oneshot::Sender<()>,
        #[allow(dead_code)]
        join_handle: tokio::task::JoinHandle<()>,
    }

    impl OIDCServerHandle {
        pub async fn start_new(jwks_json: String) -> anyhow::Result<Self> {
            let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
            let addr = listener.local_addr()?;
            let port = addr.port();
            let base_url = format!("http://localhost:{port}");
            let config = OIDCConfig {
                jwks_uri: format!("{base_url}/jwks.json"),
            };
            let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

            let app = Router::new()
                .route(
                    "/.well-known/openid-configuration",
                    get({
                        let config = config.clone();
                        move || oidc_config_handler(config.clone())
                    }),
                )
                .route(
                    "/jwks.json",
                    get({
                        let jwks = jwks_json.clone();
                        move || async move { jwks }
                    }),
                )
                .route("/ok", get(|| async move { "OK" }));

            // Spawn the server in a background task
            let join_handle = tokio::spawn(async move {
                axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        shutdown_rx.await.ok();
                    })
                    .await
                    .unwrap();
            });

            // Wait for server to be ready
            let client = reqwest::Client::new();
            let health_check_url = format!("{base_url}/ok");

            let mut attempts = 0;
            const MAX_ATTEMPTS: u32 = 10;
            const DELAY_MS: u64 = 50;

            while attempts < MAX_ATTEMPTS {
                match client.get(&health_check_url).send().await {
                    Ok(response) if response.status().is_success() => break,
                    _ => {
                        log::debug!("Server not ready. Waiting...");
                        tokio::time::sleep(Duration::from_millis(DELAY_MS)).await;
                        attempts += 1;
                    }
                }
            }

            if attempts == MAX_ATTEMPTS {
                return Err(anyhow::anyhow!("Server failed to start after maximum attempts"));
            }

            Ok(OIDCServerHandle {
                base_url,
                shutdown_tx,
                join_handle,
            })
        }
    }

    #[derive(Debug, Copy, Clone)]
    struct TestOptions {
        pub issuer_trailing_slash: bool,
        pub jwks_key_ids: bool,
    }

    impl Default for TestOptions {
        fn default() -> Self {
            Self {
                issuer_trailing_slash: false,
                jwks_key_ids: true,
            }
        }
    }

    async fn run_oidc_test<T: TokenValidator>(validator: T, opts: &TestOptions) -> anyhow::Result<()> {
        // We will put 2 keys in the keyset.
        let mut kp1 = JwtKeys::generate()?;
        let mut kp2 = JwtKeys::generate()?;

        if opts.jwks_key_ids {
            kp1.kid = Some("key1".to_string());
            kp2.kid = Some("key2".to_string());
        }

        // We won't put this in the keyset.
        let invalid_kp = JwtKeys::generate()?;

        let valid_keys: Vec<JwtKeys> = vec![kp1.clone(), kp2.clone()];
        // let jwks = keyset_to_json(vec![&jk, &kp1])?;
        let jwks = keyset_to_json(valid_keys)?;

        let handle = OIDCServerHandle::start_new(jwks).await?;

        let issuer = handle.base_url.clone();
        let issuer = if opts.issuer_trailing_slash {
            format!("{issuer}/")
        } else {
            issuer
        };
        let subject = "test_subject";

        let orig_claims = IncomingClaims {
            identity: None,
            subject: subject.into(),
            issuer: issuer.clone().into(),
            audience: [].into(),
            iat: std::time::SystemTime::now(),
            exp: None,
            extra: None,
        };
        for kp in [kp1, kp2] {
            log::debug!("Testing with key {:?}", kp.kid);
            // TODO: This test should also try using key ids in the token headers.
            let token = kp.private.sign(&orig_claims)?;

            let validated_claims = validator.validate_token(&token).await?;
            assert_eq!(&*validated_claims.issuer, &*issuer);
            assert_eq!(&*validated_claims.subject, subject);
            assert_eq!(validated_claims.identity, Identity::from_claims(&issuer, subject));
        }

        let invalid_token = invalid_kp.private.sign(&orig_claims)?;
        assert!(validator.validate_token(&invalid_token).await.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_oidc_flow() -> anyhow::Result<()> {
        for _ in 0..10 {
            run_oidc_test(OidcTokenValidator, &Default::default()).await?
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_issuer_slash() -> anyhow::Result<()> {
        let opts = TestOptions {
            issuer_trailing_slash: true,
            ..Default::default()
        };

        run_oidc_test(OidcTokenValidator, &opts).await?;
        run_oidc_test(CachingOidcTokenValidator::get_default(), &opts).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_oidc_flow_without_jwks_key_ids() -> anyhow::Result<()> {
        let opts = TestOptions {
            jwks_key_ids: false,
            ..Default::default()
        };

        run_oidc_test(OidcTokenValidator, &opts).await?;
        run_oidc_test(CachingOidcTokenValidator::get_default(), &opts).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_caching_oidc_flow() -> anyhow::Result<()> {
        for _ in 0..10 {
            let v = CachingOidcTokenValidator::get_default();
            run_oidc_test(v, &Default::default()).await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_full_validator_fallback() -> anyhow::Result<()> {
        let kp = JwtKeys::generate()?;
        let v = FullTokenValidator {
            local_key: kp.public,
            local_issuer: "local_issuer".into(),
            oidc_validator: OidcTokenValidator,
        };
        run_oidc_test(v, &Default::default()).await
    }

    /// Convert a set of keys to a JWKS JSON string.
    fn keyset_to_json<I>(jks: I) -> anyhow::Result<String>
    where
        I: IntoIterator<Item = JwtKeys>,
    {
        let jks = jks
            .into_iter()
            .map(|key| to_jwk_json(&key).unwrap())
            .collect::<Vec<serde_json::Value>>();

        let j = serde_json::json!({
            "keys": jks,
        });
        Ok(j.to_string())
    }

    // Extract the x and y coordinates from a public key and return a JWK for a single key.
    fn to_jwk_json(jk: &JwtKeys) -> anyhow::Result<serde_json::Value> {
        let eck = EcKey::public_key_from_pem(&jk.public_pem)?;

        let group = EcGroup::from_curve_name(openssl::nid::Nid::X9_62_PRIME256V1)?;
        let mut ctx = openssl::bn::BigNumContext::new()?;

        // Get the x and y coordinates.
        let mut x = openssl::bn::BigNum::new()?;
        // let mut x = openssl::bn::BigNumRef
        let mut y = openssl::bn::BigNum::new()?;
        eck.public_key().affine_coordinates(&group, &mut x, &mut y, &mut ctx)?;

        let x_bytes = x.to_vec();
        let y_bytes = y.to_vec();

        let x_padded = if x_bytes.len() < 32 {
            let mut padded = vec![0u8; 32];
            padded[32 - x_bytes.len()..].copy_from_slice(&x_bytes);
            padded
        } else {
            x_bytes
        };

        let y_padded = if y_bytes.len() < 32 {
            let mut padded = vec![0u8; 32];
            padded[32 - y_bytes.len()..].copy_from_slice(&y_bytes);
            padded
        } else {
            y_bytes
        };
        let x_b64 = base64::prelude::BASE64_URL_SAFE_NO_PAD.encode(x_padded);
        let y_b64 = base64::prelude::BASE64_URL_SAFE_NO_PAD.encode(y_padded);

        let mut jwks = serde_json::json!(
            {
                "kty": "EC",
                "crv": "P-256",
                "use": "sig",
                "alg": "ES256",
                "x": x_b64,
                "y": y_b64
            }
        );
        if let Some(kid) = &jk.kid {
            jwks["kid"] = kid.to_string().into();
        }
        Ok(jwks)
    }
}
