use crate::identity::Identity;
pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::errors::ErrorKind as JwtErrorKind;
use jsonwebtoken::{decode, encode, Header, TokenData, Validation};
pub use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
    decode::<SpacetimeIdentityClaims>(token, public_key, &validation)
}
