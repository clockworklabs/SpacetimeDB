use crate::hash::Hash;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, TokenData, Validation};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::SystemTime};

#[derive(Debug, Serialize, Deserialize)]
pub struct SpacetimeIdentityClaims {
    pub hex_identity: String,
    pub iat: usize,
}

const PRIVATE_KEY_PEM: &[u8] = include_bytes!("./id_ecdsa");
static PRIVATE_KEY: Lazy<EncodingKey> = Lazy::new(|| EncodingKey::from_ec_pem(PRIVATE_KEY_PEM).unwrap());

const PUBLIC_KEY_PEM: &[u8] = include_bytes!("./id_ecdsa.pub");
static PUBLIC_KEY: Lazy<DecodingKey> = Lazy::new(|| DecodingKey::from_ec_pem(PUBLIC_KEY_PEM).unwrap());

pub fn encode_token(identity: Hash) -> Result<String, jsonwebtoken::errors::Error> {
    let header = Header::new(jsonwebtoken::Algorithm::ES256);
    let hex_identity = identity.to_hex();
    let claims = SpacetimeIdentityClaims {
        hex_identity,
        iat: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize,
    };
    encode(&header, &claims, &PRIVATE_KEY)
}

pub fn decode_token(token: &str) -> Result<TokenData<SpacetimeIdentityClaims>, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(jsonwebtoken::Algorithm::ES256);
    validation.required_spec_claims = HashSet::new();
    decode::<SpacetimeIdentityClaims>(token, &PUBLIC_KEY, &validation)
}
