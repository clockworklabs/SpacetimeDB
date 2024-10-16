use crate::identity::Identity;
use jsonwebtoken::decode_header;
pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::errors::ErrorKind as JwtErrorKind;
use jsonwebtoken::{decode, encode, Header, TokenData, Validation};
pub use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::{Deserialize, Serialize};
use sha1::digest::generic_array::arr::Inc;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

#[serde_with::serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct SpacetimeIdentityClaims {
    #[serde(rename = "hex_identity")]
    pub identity: Identity,
    /// The unix timestamp the token was issued at
    #[serde_as(as = "serde_with::TimestampSeconds")]
    pub iat: SystemTime,
    #[serde_as(as = "Option<serde_with::TimestampSeconds>")]
    pub exp: Option<SystemTime>,
}

// The new token format that we are sending out.
#[serde_with::serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct SpacetimeIdentityClaims2 {
    #[serde(rename = "hex_identity")]
    pub identity: Identity,
    #[serde(rename = "sub")]
    pub subject: String,
    #[serde(rename = "iss")]
    pub issuer: String,
    #[serde(rename = "aud")]
    pub audience: Vec<String>,

    /// The unix timestamp the token was issued at
    #[serde_as(as = "serde_with::TimestampSeconds")]
    pub iat: SystemTime,
    #[serde_as(as = "Option<serde_with::TimestampSeconds>")]
    pub exp: Option<SystemTime>,
}

impl Into<SpacetimeIdentityClaims> for SpacetimeIdentityClaims2 {
    fn into(self) -> SpacetimeIdentityClaims {
        SpacetimeIdentityClaims {
            identity: self.identity,
            iat: self.iat,
            exp: self.exp,
        }
    }
}

// IncomingClaims are from the token we receive from the client.
// The signature should be verified already, but further validation is needed to have a SpacetimeIdentityClaims2.
#[serde_with::serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct IncomingClaims {
    #[serde(rename = "hex_identity")]
    pub identity: Option<Identity>,
    #[serde(rename = "sub")]
    pub subject: String,
    #[serde(rename = "iss")]
    pub issuer: String,
    #[serde(rename = "aud", default)]
    pub audience: Vec<String>,

    /// The unix timestamp the token was issued at
    #[serde_as(as = "serde_with::TimestampSeconds")]
    pub iat: SystemTime,
    #[serde_as(as = "Option<serde_with::TimestampSeconds>")]
    pub exp: Option<SystemTime>,
}

impl TryInto<SpacetimeIdentityClaims2> for IncomingClaims {
    type Error = TokenValidationError;

    fn try_into(self) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        // The issuer and subject must be less than 128 bytes.
        if self.issuer.len() > 128 {
            return Err(TokenValidationError::Other(anyhow::anyhow!(
                "Issuer too long: {:?}",
                self.issuer
            )));
        }
        if self.subject.len() > 128 {
            return Err(TokenValidationError::Other(anyhow::anyhow!(
                "Subject too long: {:?}",
                self.subject
            )));
        }
        // The issuer and subject must be non-empty.
        if self.issuer.is_empty() {
            return Err(TokenValidationError::Other(anyhow::anyhow!("Issuer empty")));
        }
        if self.subject.is_empty() {
            return Err(TokenValidationError::Other(anyhow::anyhow!("Subject empty")));
        }

        let computed_identity = Identity::from_claims(&self.issuer, &self.subject);
        // If an identity is provided, it must match the computed identity.
        if let Some(token_identity) = self.identity {
            if token_identity != computed_identity {
                return Err(TokenValidationError::Other(anyhow::anyhow!(
                    "Identity mismatch: token identity {:?} does not match computed identity {:?}",
                    token_identity,
                    computed_identity
                )));
            }
        }

        Ok(SpacetimeIdentityClaims2 {
            identity: computed_identity,
            subject: self.subject,
            issuer: self.issuer,
            audience: self.audience,
            iat: self.iat,
            exp: self.exp,
        })
    }
}

/// Encode a JWT token using a private_key and an identity. Expiry is set in absolute seconds,
/// the function will calculate a proper duration since unix epoch
pub fn encode_token(private_key: &EncodingKey, identity: Identity) -> Result<String, JwtError> {
    encode_token_with_expiry(private_key, identity, None)
}

pub fn encode_token_with_expiry(
    private_key: &EncodingKey,
    identity: Identity,
    expiry: Option<Duration>,
) -> Result<String, JwtError> {
    let header = Header::new(jsonwebtoken::Algorithm::ES256);

    let now = SystemTime::now();

    let expiry = expiry.map(|dur| now + dur);

    let claims = SpacetimeIdentityClaims {
        identity,
        iat: now,
        exp: expiry,
    };
    encode(&header, &claims, private_key)
}

pub fn decode_token(public_key: &DecodingKey, token: &str) -> Result<TokenData<SpacetimeIdentityClaims>, JwtError> {
    let mut validation = Validation::new(jsonwebtoken::Algorithm::ES256);
    validation.required_spec_claims = HashSet::new();
    // TODO: This should be fixed.
    validation.validate_aud = false;
    decode::<SpacetimeIdentityClaims>(token, public_key, &validation)
}

use anyhow;
use thiserror;

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

use async_trait::async_trait;

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
        self.validate_token(token).await
    }
}

pub struct UnimplementedTokenValidator;

#[async_trait]
impl TokenValidator for UnimplementedTokenValidator {
    async fn validate_token(&self, _token: &str) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        Err(TokenValidationError::Other(anyhow::anyhow!("Unimplemented")))
    }
}

pub struct InitialTestingTokenValidator {
    pub public_key: DecodingKey,
}

#[async_trait]
impl TokenValidator for InitialTestingTokenValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        let issuer = get_raw_issuer(token)?;
        if issuer == "localhost" {
            let claims = LocalTokenValidator {
                public_key: self.public_key.clone(),
                issuer,
            }
            .validate_token(token)
            .await?;
            return Ok(claims);
        }
        let validator = OidcTokenValidator;
        validator.validate_token(token).await
    }
}

// This verifies against a given public key and expected issuer.
struct LocalTokenValidator {
    pub public_key: DecodingKey,
    pub issuer: String,
}

use lazy_static::lazy_static;

lazy_static! {
    // Eventually we will want to add more required claims.
    static ref REQUIRED_CLAIMS: Vec<&'static str> = vec!["sub", "iss"];
}

// Just make some token website thing.

// Get a minimal branch out that doesn't do any caching or fancy stuff.

#[async_trait]
impl TokenValidator for LocalTokenValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        // TODO: Make this stored in the struct so we don't need to keep creating it.
        let mut validation = Validation::new(jsonwebtoken::Algorithm::ES256);
        validation.set_required_spec_claims(&REQUIRED_CLAIMS);
        validation.set_issuer(&[self.issuer.clone()]);

        // TODO: We should require a specific audience at some point.
        validation.validate_aud = false;

        let data = decode::<IncomingClaims>(token, &self.public_key, &validation)?;
        let claims = data.claims;
        if claims.issuer != self.issuer {
            return Err(TokenValidationError::Other(anyhow::anyhow!(
                "Issuer mismatch: got {:?}, expected {:?}",
                claims.issuer,
                self.issuer
            )));
        }
        claims.try_into()
    }
}

// This is a token validator that uses OIDC to validate tokens.
// This will look up the public key for the issuer and validate against that key.
// This currently has no caching.
pub struct OidcTokenValidator;
use jwks::Jwks;

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
            let validator = LocalTokenValidator {
                public_key: key.decoding_key.clone(),
                issuer: self.issuer.clone(),
            };
            return validator.validate_token(token).await;
        }
        // TODO: Consider returning an error if no kid is given?
        // For now, lets just try all the keys.
        let mut last_error = TokenValidationError::Other(anyhow::anyhow!("No kid found"));
        for (_, key) in &self.keyset.keys {
            let validator = LocalTokenValidator {
                public_key: key.decoding_key.clone(),
                issuer: self.issuer.clone(),
            };
            match validator.validate_token(token).await {
                Ok(claims) => return Ok(claims),
                Err(e) => {
                    last_error = e;
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

    use crate::auth::identity::{IncomingClaims, LocalTokenValidator, SpacetimeIdentityClaims2, TokenValidator};
    use jsonwebkey as jwk;
    use jsonwebtoken::{DecodingKey, EncodingKey};
    use spacetimedb_lib::Identity;

    struct KeyPair {
        pub public_key: DecodingKey,
        pub private_key: EncodingKey,
    }

    fn new_keypair() -> anyhow::Result<KeyPair> {
        let mut my_jwk = jwk::JsonWebKey::new(jwk::Key::generate_p256());

        my_jwk.set_algorithm(jwk::Algorithm::ES256).unwrap();
        let public_key = jsonwebtoken::DecodingKey::from_ec_pem(&my_jwk.key.to_public().unwrap().to_pem().as_bytes())?;
        let private_key = jsonwebtoken::EncodingKey::from_ec_pem(&my_jwk.key.try_to_pem()?.as_bytes())?;
        Ok(KeyPair {
            public_key,
            private_key,
        })
    }

    #[tokio::test]
    async fn test_local_validator_checks_issuer() -> anyhow::Result<()> {
        // Test that the issuer must match the expected issuer for LocalTokenValidator.
        let kp = new_keypair()?;
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
            let validator = LocalTokenValidator {
                public_key: kp.public_key.clone(),
                issuer: issuer.to_string(),
            };

            let parsed_claims: SpacetimeIdentityClaims2 = validator.validate_token(&token).await?;
            assert_eq!(parsed_claims.issuer, issuer);
            assert_eq!(parsed_claims.subject, subject);
            assert_eq!(parsed_claims.identity, Identity::from_claims(issuer, subject));
        }
        {
            // Now try with the wrong expected issuer.
            let validator = LocalTokenValidator {
                public_key: kp.public_key.clone(),
                issuer: "otherissuer".to_string(),
            };

            assert!(validator.validate_token(&token).await.is_err());
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_local_validator_checks_key() -> anyhow::Result<()> {
        // Test that the decoding key must work for LocalTokenValidator.
        let kp = new_keypair()?;
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
            let validator = LocalTokenValidator {
                public_key: kp.public_key.clone(),
                issuer: issuer.to_string(),
            };

            let parsed_claims: SpacetimeIdentityClaims2 = validator.validate_token(&token).await?;
            assert_eq!(parsed_claims.issuer, issuer);
            assert_eq!(parsed_claims.subject, subject);
            assert_eq!(parsed_claims.identity, Identity::from_claims(issuer, subject));
        }
        {
            // We generate a new keypair and try to decode with that key.
            let other_kp = new_keypair()?;
            // Now try with the wrong expected issuer.
            let validator = LocalTokenValidator {
                public_key: other_kp.public_key.clone(),
                issuer: "otherissuer".to_string(),
            };

            assert!(validator.validate_token(&token).await.is_err());
        }

        Ok(())
    }

    // use jsonwebtoken::jwk::JwkSet
    use axum::http::{Request, StatusCode};
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

    // TODO: Finish this test.
    #[tokio::test]
    async fn test_oidc_flow() -> anyhow::Result<()> {
        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        let addr = listener.local_addr()?;
        let port = addr.port();
        let base_url = format!("http://localhost:{}", port);

        let config = OIDCConfig {
            jwks_uri: format!("{}/jwks", base_url),
        };
        let url_clone = base_url.clone();
        // let config_thing = Json(config);
        // let app = Router::new()
        // // .route("/", get(|| async move { url_clone.clone() }))
        // .route("/.well-known/openid-configuration", get(config_thing));

        let app = Router::new().route(
            "/.well-known/openid-configuration",
            get({
                let config = config.clone();
                move || oidc_config_handler(config.clone())
            }),
        );
        // let idk = axum::serve(listener, app).await?;

        // Create a shutdown channel
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        // Spawn the server in a background task
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                })
                .await
                .unwrap();
        });

        // tokio::spawn(server);

        Ok(())
    }
}
