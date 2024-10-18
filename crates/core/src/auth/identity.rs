use crate::identity::Identity;
use jsonwebtoken::decode_header;
pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::errors::ErrorKind as JwtErrorKind;
use jsonwebtoken::{decode, encode, Header, TokenData, Validation};
pub use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::{Deserialize, Serialize};
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


type CacheValue = Arc<JwksValidator>;

pub struct CachingOidcTokenValidator {
    cache: async_cache::AsyncCache<Arc<JwksValidator>, KeyFetcher>,
}

// pub struct CachingOidcTokenValidator<T: async_cache::Fetcher<Arc<JwksValidator>> + Sync + Send + 'static> {
//     cache: async_cache::AsyncCache<Arc<JwksValidator>, T>,
// }


impl CachingOidcTokenValidator {

    fn new(refresh_duration: Duration, expiry: Option<Duration>) -> Self {
        let cache = async_cache::Options::new(refresh_duration, KeyFetcher)
            .with_expire(expiry)
            .build();
        CachingOidcTokenValidator {
            cache
        }
    }

    fn default() -> Self {
        Self::new(Duration::from_secs(300), Some(Duration::from_secs(7200)))
    }
}

struct KeyFetcher;

use async_cache;
use faststr::FastStr;

impl async_cache::Fetcher<Arc<JwksValidator>> for KeyFetcher {
    type Error = TokenValidationError;

    async fn fetch(&self, key: FastStr) -> Result<Arc<JwksValidator>, Self::Error> {
        // TODO: Make this stored in the struct so we don't need to keep creating it.
        // let raw_issuer = get_raw_issuer(token)?;
        let raw_issuer = key.to_string();
        println!("Fetching key for issuer {}", raw_issuer.clone());
        // TODO: Consider checking for trailing slashes or requiring a scheme.
        let oidc_url = format!("{}/.well-known/openid-configuration", raw_issuer);
        // TODO: log errors here.
        let keys = Jwks::from_oidc_url(oidc_url).await?;
        let validator = JwksValidator {
            issuer: raw_issuer.clone(),
            keyset: keys,
        };
        println!("Built validator for issuer {}", raw_issuer.clone());
        Ok(Arc::new(validator))
    }
}

#[async_trait]
impl TokenValidator for CachingOidcTokenValidator {
    async fn validate_token(&self, token: &str) -> Result<SpacetimeIdentityClaims2, TokenValidationError> {
        println!("Validating token");
        let raw_issuer = get_raw_issuer(token)?;
        let validator = self.cache.get(raw_issuer.clone().into()).await.ok_or_else(|| { anyhow::anyhow!("Error fetching public key for issuer {}", raw_issuer)})?;
        // Err(anyhow::anyhow!("Dummy error"))
        // validator.validate_token(token).await
        Result::Err(TokenValidationError::Other(anyhow::anyhow!("Dummy error")))
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

    use std::sync::Arc;

    use crate::auth::identity::{
        IncomingClaims, LocalTokenValidator, OidcTokenValidator, SpacetimeIdentityClaims2, TokenValidator, CachingOidcTokenValidator
    };
    use jsonwebkey as jwk;
    use jsonwebtoken::{DecodingKey, EncodingKey};
    use rand::distributions::{Alphanumeric, DistString};
    use rand::{thread_rng, Rng};
    use serde_json;
    use spacetimedb_lib::Identity;

    struct KeyPair {
        pub public_key: DecodingKey,
        pub private_key: EncodingKey,
        pub key_id: String,
        // pub jwks: String,
        pub jwk: jwk::JsonWebKey,
    }

    fn to_jwks_json<I, K>(keys: I) -> String
    where
        I: IntoIterator<Item = K>,
        K: AsRef<KeyPair>,
        // I: IntoIterator<Item = KeyPair>,
    {
        format!(
            r#"{{"keys":[{}]}}"#,
            keys.into_iter()
                .map(|key| serde_json::to_string(&key.as_ref().to_public()).unwrap())
                .collect::<Vec<String>>()
                .join(",")
        )
    }

    struct KeySet {
        pub keys: Vec<KeyPair>,
    }

    impl KeyPair {
        fn new(key_id: String, key: jwk::JsonWebKey) -> anyhow::Result<Self> {
            let public_key = jsonwebtoken::DecodingKey::from_ec_pem(&key.key.to_public().unwrap().to_pem().as_bytes())?;
            let private_key = jsonwebtoken::EncodingKey::from_ec_pem(&key.key.try_to_pem()?.as_bytes())?;
            Ok(KeyPair {
                public_key,
                private_key,
                key_id,
                jwk: key,
            })
        }

        fn generate_p256() -> anyhow::Result<KeyPair> {
            let key_id = Alphanumeric.sample_string(&mut thread_rng(), 16);
            let mut my_jwk = jwk::JsonWebKey::new(jwk::Key::generate_p256());
            my_jwk.key_id = Some(key_id.clone());
            my_jwk.set_algorithm(jwk::Algorithm::ES256).unwrap();
            let public_key =
                jsonwebtoken::DecodingKey::from_ec_pem(&my_jwk.key.to_public().unwrap().to_pem().as_bytes())?;
            let private_key = jsonwebtoken::EncodingKey::from_ec_pem(&my_jwk.key.try_to_pem()?.as_bytes())?;
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

        fn to_public_jwks(&self) -> String {
            let mut public_only = self.jwk.clone();
            public_only.key = Box::from(public_only.key.to_public().unwrap().into_owned());
            format!(r#"{{"keys":[{}]}}"#, serde_json::to_string(&self.jwk).unwrap())
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
            let other_kp = KeyPair::generate_p256()?;
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

    // You can drop this to shut down the server.
    // This will host an oidc config at `{base_url}/.well-known/openid-configuration`
    // It will also host jwks at `{base_url}/jwks.json`
    struct OIDCServerHandle {
        pub base_url: String,
        pub shutdown_tx: oneshot::Sender<()>,
        join_handle: tokio::task::JoinHandle<()>,
    }

    impl OIDCServerHandle {
        pub async fn start_new<I, K>(ks: I) -> anyhow::Result<Self>
        where
            // Keys: IntoIterator<Item = AsRef<KeyPair>>,
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
                        move || async move {
                            jwks
                            //Json(kp.to_public_jwks())
                        }
                    }),
                )
                .route("/ok", get(|| async move { "OK" }));

            // Spawn the server in a background task
            let join_handle = tokio::spawn(async move {
                axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        shutdown_rx.await.ok();
                        println!("Shutting down");
                    })
                    .await
                    .unwrap();
                println!("Server shut down");
            });

            Ok(OIDCServerHandle {
                base_url,
                shutdown_tx, /* , join_handle*/
                join_handle,
            })
        }
    }

    #[tokio::test]
    async fn test_oidc_flow() -> anyhow::Result<()> {
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
            let token = jsonwebtoken::encode(&header, &orig_claims, &kp.private_key)?;

            let validated_claims = OidcTokenValidator.validate_token(&token).await?;
            assert_eq!(validated_claims.issuer, issuer);
            assert_eq!(validated_claims.subject, subject);
            assert_eq!(validated_claims.identity, Identity::from_claims(&issuer, subject));
        }

        let invalid_token = jsonwebtoken::encode(&header, &orig_claims, &invalid_kp.private_key)?;
        assert!(OidcTokenValidator.validate_token(&invalid_token).await.is_err());
        // tokio::spawn(server);

        Ok(())
    }

    #[tokio::test]
    async fn test_oidc_caching_flow() -> anyhow::Result<()> {
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
            let token = jsonwebtoken::encode(&header, &orig_claims, &kp.private_key)?;

            let validated_claims = OidcTokenValidator.validate_token(&token).await?;
            assert_eq!(validated_claims.issuer, issuer);
            assert_eq!(validated_claims.subject, subject);
            assert_eq!(validated_claims.identity, Identity::from_claims(&issuer, subject));
        }

        let invalid_token = jsonwebtoken::encode(&header, &orig_claims, &invalid_kp.private_key)?;
        let validator = CachingOidcTokenValidator::default();
        println!("Starting invalid token test");
        assert!(validator.validate_token(&invalid_token).await.is_err());
        // tokio::spawn(server);

        Ok(())
    }
}
