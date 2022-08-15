use crate::hash::Hash;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, TokenData, Validation};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::SystemTime};

#[derive(Debug, Serialize, Deserialize)]
pub struct SpacetimeIdentityClaims {
    pub hex_identity: String,
    pub iat: usize,
}

const PRIVATE_KEY: &'static [u8; 240] = b"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgfv97uvAWHCwiUozf
8Qu6yHFpmV7Tx27QTjwY/BU9ZxKhRANCAATKxjFoZkGB6ih2SQdeG7KtyBVujSp7
JChJw40MnxgBExJMZv3xDpfPNFChUDgtkMGqQS1OhOLtExrmdUNe7ySb
-----END PRIVATE KEY-----";

const PUBLIC_KEY: &'static [u8; 177] = b"-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEysYxaGZBgeoodkkHXhuyrcgVbo0q
eyQoScONDJ8YARMSTGb98Q6XzzRQoVA4LZDBqkEtToTi7RMa5nVDXu8kmw==
-----END PUBLIC KEY-----";

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
    encode(&header, &claims, &EncodingKey::from_ec_pem(PRIVATE_KEY).unwrap())
}

pub fn decode_token(token: &str) -> Result<TokenData<SpacetimeIdentityClaims>, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(jsonwebtoken::Algorithm::ES256);
    validation.required_spec_claims = HashSet::new();
    decode::<SpacetimeIdentityClaims>(token, &DecodingKey::from_ec_pem(PUBLIC_KEY).unwrap(), &validation)
}
