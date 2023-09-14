use crate::identity::Identity;
use jsonwebtoken::{decode, encode, Header, TokenData, Validation};
use serde::{Deserialize, Serialize};
use std::time::{Duration, UNIX_EPOCH};
use std::{collections::HashSet, time::SystemTime};

pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::errors::ErrorKind as JwtErrorKind;
pub use jsonwebtoken::{DecodingKey, EncodingKey};

#[serde_with::serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct SpacetimeIdentityClaims {
    #[serde(rename = "hex_identity")]
    pub identity: Identity,
    #[serde_as(as = "serde_with::TimestampSeconds")]
    pub iat: SystemTime,
    pub exp: Option<u64>,
}

/// Encode a JWT token using a private_key and an identity. Expiry is set in absolute seconds,
/// the function will calculate a proper duration since unix epoch
pub fn encode_token(private_key: &EncodingKey, identity: Identity) -> Result<String, JwtError> {
    encode_token_with_expiry(private_key, identity, None)
}

pub fn encode_token_with_expiry(
    private_key: &EncodingKey,
    identity: Identity,
    expiry: Option<u64>,
) -> Result<String, JwtError> {
    let header = Header::new(jsonwebtoken::Algorithm::ES256);

    let expiry = expiry.map(|seconds| {
        let mut timer = SystemTime::now();
        timer += Duration::from_secs(seconds);
        // SAFETY: duration_since will panic if an argument is later than the time
        // used for the duration calculation. In case of UNIX_EPOCH it can't be the case
        timer.duration_since(UNIX_EPOCH).unwrap().as_secs()
    });

    let claims = SpacetimeIdentityClaims {
        identity,
        iat: SystemTime::now(),
        exp: expiry,
    };
    encode(&header, &claims, private_key)
}

pub fn decode_token(public_key: &DecodingKey, token: &str) -> Result<TokenData<SpacetimeIdentityClaims>, JwtError> {
    let mut validation = Validation::new(jsonwebtoken::Algorithm::ES256);
    validation.required_spec_claims = HashSet::new();
    decode::<SpacetimeIdentityClaims>(token, public_key, &validation)
}
