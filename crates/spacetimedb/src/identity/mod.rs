use crate::{hash::{Hash, hash_bytes}, postgres};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, TokenData, Validation};
use lazy_static::lazy_static;
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SpacetimeIdentityClaims {
    pub hex_identity: String,
    pub iat: usize,
}

lazy_static! {
    static ref SIGNING_SECRET: String = "This is a secret yo.".into();
}

pub async fn alloc_spacetime_identity() -> Result<Hash, anyhow::Error> {
    // TODO: this really doesn't need to be a single global count
    let client = postgres::get_client().await;
    let result = client.query(
        "INSERT INTO registry.st_identity (num) VALUES (0) ON CONFLICT (onerow_id) DO UPDATE SET num = st_identity.num + 1 RETURNING num",
        &[]
    ).await?;
    let row = result.first().unwrap();
    let count: i32 = row.get(0);
    let bytes: &[u8] = &count.to_le_bytes();
    let name = b"clockworklabs:";
    let bytes = [name, bytes].concat();
    let hash = hash_bytes(bytes);
    Ok(hash)
}

pub fn encode_token(identity: Hash) -> Result<String, jsonwebtoken::errors::Error> {
    let header = Header::new(jsonwebtoken::Algorithm::ES256);
    let hex_identity = hex::encode(identity);
    let claims = SpacetimeIdentityClaims {
        hex_identity,
        iat: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as usize,
    };
    encode(&header, &claims, &EncodingKey::from_secret(SIGNING_SECRET.as_ref()))
}

pub fn decode_token(token: &str) -> Result<TokenData<SpacetimeIdentityClaims>, jsonwebtoken::errors::Error> {
    let validation = Validation::new(jsonwebtoken::Algorithm::ES256);
    decode::<SpacetimeIdentityClaims>(
        token,
        &DecodingKey::from_secret(SIGNING_SECRET.as_ref()),
        &validation,
    )
}
