use anyhow;
use async_cache;
use async_trait::async_trait;
use faststr::FastStr;
use jsonwebtoken::decode_header;
pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::errors::ErrorKind as JwtErrorKind;
use jsonwebtoken::{decode, Validation};
pub use jsonwebtoken::{DecodingKey, EncodingKey};
use jwks::Jwks;
use lazy_static::lazy_static;
use std::sync::Arc;
use std::time::Duration;
use thiserror;

use super::identity::{IncomingClaims, SpacetimeIdentityClaims2};

#[derive(thiserror::Error, Debug)]
pub enum TokenValidationError {
    // TODO: Add real error types.

    // TODO: If we had our own errors defined we wouldn't be locked into this lib.
    #[error("Invalid token: {0}")]
    TokenError(#[from] JwtError),

    #[error("Specified key ID not found in JWKs")]
    KeyIDNotFound,

    #[error(transparent)]
    JwkError(#[from] jwks::JwkError),
    #[error(transparent)]
    JwksError(#[from] jwks::JwksError),
    // The other case is a catch-all for unexpected errors.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

// A TokenValidator is responsible for validating a token and returning the claims.
// This includes looking up the public key for the issuer and verifying the signature.
// It is also responsible for enforcing the rules around the claims.
// For example, this must ensure that the issuer and sub are no longer than 128 bytes.
#[async_trait]
pub trait TokenValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims2, TokenValidationError>;
}

#[async_trait]
impl<T: TokenValidator + Send + Sync> TokenValidator for Arc<T> {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        (**self).validate_token(token).await
    }
}

pub struct UnimplementedTokenValidator;

#[async_trait]
impl TokenValidator for UnimplementedTokenValidator {
    async fn validate_token(&self, _token: &str) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        Err(TokenValidationError::Other(anyhow::anyhow!("Unimplemented")))
    }
}

/* 
pub struct FullTokenValidator {
    pub public_key: DecodingKey,
    pub caching_validator: CachingOidcTokenValidator,
}

#[async_trait]
impl TokenValidator for FullTokenValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        let issuer = get_raw_issuer(token)?;
        if issuer == "localhost" {
            let claims = BasicTokenValidator {
                public_key: self.public_key.clone(),
                issuer,
            }
            .validate_token(token)
            .await?;
            return Ok(claims);
        }
        self.caching_validator.validate_token(token).await
    }
}
    */

pub async fn validate_token(
    local_key: DecodingKey,
    local_issuer: &str,
    token: &str,
) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        let local_key_error = {

            let first_validator = BasicTokenValidator {
                public_key: local_key.clone(),
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
        if issuer == local_issuer {
            return Err(local_key_error);
        }
        GLOBAL_OIDC_VALIDATOR.clone().validate_token(token).await
}

pub struct InitialTestingTokenValidator {
    pub public_key: DecodingKey,
}

#[async_trait]
impl TokenValidator for InitialTestingTokenValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        // Initially, we check if we signed the key.
        let local_key_error = {

            let first_validator = BasicTokenValidator {
                public_key: self.public_key.clone(),
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
        if issuer == "localhost" {
            return Err(local_key_error);
        }
        let validator = OidcTokenValidator;
        validator.validate_token(token).await
    }
}

// This verifies against a given public key and expected issuer.
// The issuer should only be None if we are checking with a local key.
// We do that because we signed short-lived keys with different issuers.
struct BasicTokenValidator {
    pub public_key: DecodingKey,
    pub issuer: Option<String>,
}

lazy_static! {
    // Eventually we will want to add more required claims.
    static ref REQUIRED_CLAIMS: Vec<&'static str> = vec!["sub", "iss"];
}

lazy_static! {
    // Eventually we will want to add more required claims.
    static ref GLOBAL_OIDC_VALIDATOR: Arc<CachingOidcTokenValidator> = Arc::new(CachingOidcTokenValidator::get_default());
}

#[async_trait]
impl TokenValidator for BasicTokenValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        // TODO: Make this stored in the struct so we don't need to keep creating it.
        let mut validation = Validation::new(jsonwebtoken::Algorithm::ES256);
        validation.algorithms = vec![
            jsonwebtoken::Algorithm::ES256,
            jsonwebtoken::Algorithm::RS256,
            jsonwebtoken::Algorithm::HS256,
        ];
        validation.set_required_spec_claims(&REQUIRED_CLAIMS);

        if let Some(expected_issuer) = &self.issuer {
            validation.set_issuer(&[expected_issuer.clone()]);
        }

        // TODO: We should require a specific audience at some point.
        validation.validate_aud = false;

        let data = decode::<IncomingClaims>(token, &self.public_key, &validation)?;
        let claims = data.claims;
        if let Some(expected_issuer) = &self.issuer {
            if claims.issuer != *expected_issuer {
                return Err(TokenValidationError::Other(anyhow::anyhow!(
                    "Issuer mismatch: got {:?}, expected {:?}",
                    claims.issuer,
                    expected_issuer
                )));
            }

        } 
        claims.try_into()
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
        let raw_issuer = key.to_string();
        log::info!("Fetching key for issuer {}", raw_issuer.clone());
        // TODO: Consider checking for trailing slashes or requiring a scheme.
        let oidc_url = format!("{}/.well-known/openid-configuration", raw_issuer);
        // TODO: log errors here.
        let keys = Jwks::from_oidc_url(oidc_url).await?;
        let validator = JwksValidator {
            issuer: raw_issuer.clone(),
            keyset: keys,
        };
        Ok(Arc::new(validator))
    }
}

#[async_trait]
impl TokenValidator for CachingOidcTokenValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        let raw_issuer = get_raw_issuer(token)?;
        log::debug!("Getting validator for issuer {}", raw_issuer.clone());
        let validator = self
            .cache
            .get(raw_issuer.clone().into())
            .await
            .ok_or_else(|| anyhow::anyhow!("Error fetching public key for issuer {}", raw_issuer))?;
        validator.validate_token(token).await
    }
}

// This is a token validator that uses OIDC to validate tokens.
// This will look up the public key for the issuer and validate against that key.
// This currently has no caching.
pub struct OidcTokenValidator;

// Get the issuer out of a token without validating the signature.
fn get_raw_issuer(token: &str) -> Result<String, TokenValidationError> {
    let mut validation = Validation::new(jsonwebtoken::Algorithm::ES256);
    validation.set_required_spec_claims(&REQUIRED_CLAIMS);
    validation.validate_aud = false;
    // We are disabling signature validation, because we need to get the issuer before we can validate.
    validation.insecure_disable_signature_validation();
    let data = decode::<IncomingClaims>(token, &DecodingKey::from_secret(b"fake"), &validation)?;
    Ok(data.claims.issuer)
}

#[async_trait]
impl TokenValidator for OidcTokenValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        // TODO: Make this stored in the struct so we don't need to keep creating it.
        let raw_issuer = get_raw_issuer(token)?;
        // TODO: Consider checking for trailing slashes or requiring a scheme.
        let oidc_url = format!("{}/.well-known/openid-configuration", raw_issuer);
        let keys = Jwks::from_oidc_url(oidc_url).await?;
        let validator = JwksValidator {
            issuer: raw_issuer,
            keyset: keys,
        };
        validator.validate_token(token).await
    }
}

struct JwksValidator {
    pub issuer: String,
    pub keyset: Jwks,
}

#[async_trait]
impl TokenValidator for JwksValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        let header = decode_header(token)?;
        if let Some(kid) = header.kid {
            let key = self
                .keyset
                .keys
                .get(&kid)
                .ok_or_else(|| TokenValidationError::KeyIDNotFound)?;
            let validator = BasicTokenValidator {
                public_key: key.decoding_key.clone(),
                issuer: Some(self.issuer.clone()),
            };
            return validator.validate_token(token).await;
        }
        log::debug!("No key id in header. Trying all keys.");
        // TODO: Consider returning an error if no kid is given?
        // For now, lets just try all the keys.
        let mut last_error = TokenValidationError::Other(anyhow::anyhow!("No kid found"));
        for (kid, key) in &self.keyset.keys {
            log::debug!("Trying key {}", kid);
            let validator = BasicTokenValidator {
                public_key: key.decoding_key.clone(),
                issuer: Some(self.issuer.clone()),
            };
            match validator.validate_token(token).await {
                Ok(claims) => return Ok(claims),
                Err(e) => {
                    last_error = e;
                    log::debug!("Validating with key {} failed", kid);
                    continue;
                }
            }
        }
        // None of the keys worked.
        Err(last_error)
    }
}

#[cfg(test)]
mod tests {

    use std::sync::Arc;

    use crate::auth::identity::{IncomingClaims, SpacetimeIdentityClaims2};
    use crate::auth::token_validation::{
        CachingOidcTokenValidator, BasicTokenValidator, OidcTokenValidator, TokenValidator,
    };
    use jsonwebkey as jwk;
    use jsonwebtoken::{DecodingKey, EncodingKey};
    use rand::distributions::{Alphanumeric, DistString};
    use rand::thread_rng;
    use serde_json;
    use spacetimedb_lib::Identity;

    struct KeyPair {
        pub public_key: DecodingKey,
        pub private_key: EncodingKey,
        pub key_id: String,
        pub jwk: jwk::JsonWebKey,
    }

    fn to_jwks_json<I, K>(keys: I) -> String
    where
        I: IntoIterator<Item = K>,
        K: AsRef<KeyPair>,
    {
        format!(
            r#"{{"keys":[{}]}}"#,
            keys.into_iter()
                .map(|key| serde_json::to_string(&key.as_ref().to_public()).unwrap())
                .collect::<Vec<String>>()
                .join(",")
        )
    }

    impl KeyPair {
        fn generate_p256() -> anyhow::Result<KeyPair> {
            let key_id = Alphanumeric.sample_string(&mut thread_rng(), 16);
            let mut my_jwk = jwk::JsonWebKey::new(jwk::Key::generate_p256());
            my_jwk.key_id = Some(key_id.clone());
            my_jwk.set_algorithm(jwk::Algorithm::ES256).unwrap();
            let public_key =
                jsonwebtoken::DecodingKey::from_ec_pem(my_jwk.key.to_public().unwrap().to_pem().as_bytes())?;
            let private_key = jsonwebtoken::EncodingKey::from_ec_pem(my_jwk.key.try_to_pem()?.as_bytes())?;
            Ok(KeyPair {
                public_key,
                private_key,
                key_id,
                jwk: my_jwk,
            })
        }

        fn to_public(&self) -> jwk::JsonWebKey {
            let mut public_only = self.jwk.clone();
            public_only.key = Box::from(public_only.key.to_public().unwrap().into_owned());
            public_only
        }
    }

    #[tokio::test]
    async fn test_local_validator_checks_issuer() -> anyhow::Result<()> {
        // Test that the issuer must match the expected issuer for LocalTokenValidator.
        let kp = KeyPair::generate_p256()?;
        let issuer = "test1";
        let subject = "test_subject";

        let orig_claims = IncomingClaims {
            identity: None,
            subject: subject.to_string(),
            issuer: issuer.to_string(),
            audience: vec![],
            iat: std::time::SystemTime::now(),
            exp: None,
        };
        let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        let token = jsonwebtoken::encode(&header, &orig_claims, &kp.private_key)?;

        {
            // Test that we can validate it.
            let validator = BasicTokenValidator {
                public_key: kp.public_key.clone(),
                issuer: Some(issuer.to_string()),
            };

            let parsed_claims: SpacetimeIdentityClaims2 = validator.validate_token(&token).await?;
            assert_eq!(parsed_claims.issuer, issuer);
            assert_eq!(parsed_claims.subject, subject);
            assert_eq!(parsed_claims.identity, Identity::from_claims(issuer, subject));
        }
        {
            // Now try with the wrong expected issuer.
            let validator = BasicTokenValidator {
                public_key: kp.public_key.clone(),
                issuer: Some("otherissuer".to_string()),
            };

            assert!(validator.validate_token(&token).await.is_err());
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_local_validator_checks_key() -> anyhow::Result<()> {
        // Test that the decoding key must work for LocalTokenValidator.
        let kp = KeyPair::generate_p256()?;
        let issuer = "test1";
        let subject = "test_subject";

        let orig_claims = IncomingClaims {
            identity: None,
            subject: subject.to_string(),
            issuer: issuer.to_string(),
            audience: vec![],
            iat: std::time::SystemTime::now(),
            exp: None,
        };
        let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        let token = jsonwebtoken::encode(&header, &orig_claims, &kp.private_key)?;

        {
            // Test that we can validate it.
            let validator = BasicTokenValidator {
                public_key: kp.public_key.clone(),
                issuer: Some(issuer.to_string()),
            };

            let parsed_claims: SpacetimeIdentityClaims2 = validator.validate_token(&token).await?;
            assert_eq!(parsed_claims.issuer, issuer);
            assert_eq!(parsed_claims.subject, subject);
            assert_eq!(parsed_claims.identity, Identity::from_claims(issuer, subject));
        }
        {
            // We generate a new keypair and try to decode with that key.
            let other_kp = KeyPair::generate_p256()?;
            // Now try with the wrong expected issuer.
            let validator = BasicTokenValidator {
                public_key: other_kp.public_key.clone(),
                issuer: Some("otherissuer".to_string()),
            };

            assert!(validator.validate_token(&token).await.is_err());
        }

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
        pub async fn start_new<I, K>(ks: I) -> anyhow::Result<Self>
        where
            I: IntoIterator<Item = K>,
            K: AsRef<KeyPair>,
        {
            let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
            let addr = listener.local_addr()?;
            let port = addr.port();
            let base_url = format!("http://localhost:{}", port);
            let config = OIDCConfig {
                jwks_uri: format!("{}/jwks.json", base_url),
            };
            let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
            let jwks_json = to_jwks_json(ks);

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

            Ok(OIDCServerHandle {
                base_url,
                shutdown_tx,
                join_handle,
            })
        }
    }

    async fn run_oidc_test<T: TokenValidator>(validator: T) -> anyhow::Result<()> {
        // We will put 2 keys in the keyset.
        let kp1 = Arc::new(KeyPair::generate_p256()?);
        let kp2 = Arc::new(KeyPair::generate_p256()?);

        // We won't put this in the keyset.
        let invalid_kp = KeyPair::generate_p256()?;

        let handle = OIDCServerHandle::start_new(vec![kp1.clone(), kp2.clone()]).await?;

        let issuer = handle.base_url.clone();
        let subject = "test_subject";

        let orig_claims = IncomingClaims {
            identity: None,
            subject: subject.to_string(),
            issuer: issuer.clone(),
            audience: vec![],
            iat: std::time::SystemTime::now(),
            exp: None,
        };
        let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        for kp in [kp1, kp2] {
            log::debug!("Testing with key {}", kp.key_id);
            let token = jsonwebtoken::encode(&header, &orig_claims, &kp.private_key)?;

            let validated_claims = validator.validate_token(&token).await?;
            assert_eq!(validated_claims.issuer, issuer);
            assert_eq!(validated_claims.subject, subject);
            assert_eq!(validated_claims.identity, Identity::from_claims(&issuer, subject));
        }

        let invalid_token = jsonwebtoken::encode(&header, &orig_claims, &invalid_kp.private_key)?;
        assert!(validator.validate_token(&invalid_token).await.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_oidc_flow() -> anyhow::Result<()> {
        run_oidc_test(OidcTokenValidator).await
    }
    #[tokio::test]
    async fn test_caching_oidc_flow() -> anyhow::Result<()> {
        let v = CachingOidcTokenValidator::get_default();
        run_oidc_test(v).await
    }
}
