use crate::identity::Identity;
pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::errors::ErrorKind as JwtErrorKind;
use jsonwebtoken::{decode, encode, Header, TokenData, Validation};
pub use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;
use std::time::SystemTime;

use super::token_validation::TokenValidationError;

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

impl From<SpacetimeIdentityClaims2> for SpacetimeIdentityClaims {
    fn from(claims: SpacetimeIdentityClaims2) -> Self {
        SpacetimeIdentityClaims {
            identity: claims.identity,
            iat: claims.iat,
            exp: claims.exp,
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
