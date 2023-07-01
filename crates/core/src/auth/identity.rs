use crate::identity::Identity;
use jsonwebtoken::{decode, encode, Header, TokenData, Validation};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::SystemTime};

pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::errors::ErrorKind as JwtErrorKind;
pub use jsonwebtoken::{DecodingKey, EncodingKey};

#[serde_with::serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct SpacetimeIdentityClaims {
    pub hex_identity: Identity,
    #[serde_as(as = "serde_with::TimestampSeconds")]
    pub iat: SystemTime,
}

pub fn encode_token(private_key: &EncodingKey, identity: Identity) -> Result<String, JwtError> {
    let header = Header::new(jsonwebtoken::Algorithm::ES256);
    let claims = SpacetimeIdentityClaims {
        hex_identity: identity,
        iat: SystemTime::now(),
    };
    encode(&header, &claims, private_key)
}

pub fn decode_token(public_key: &DecodingKey, token: &str) -> Result<TokenData<SpacetimeIdentityClaims>, JwtError> {
    let mut validation = Validation::new(jsonwebtoken::Algorithm::ES256);
    validation.required_spec_claims = HashSet::new();
    decode::<SpacetimeIdentityClaims>(token, public_key, &validation)
}
