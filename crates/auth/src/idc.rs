//! Host-signed transport credentials. These are never exposed as module JWTs.

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use spacetimedb_lib::Identity;
use std::time::{SystemTime, UNIX_EPOCH};

const ISSUER: &str = "spacetimedb-idc-v1";
const TOKEN_LIFETIME_SECS: u64 = 60;

#[derive(Serialize, Deserialize)]
struct Claims {
    iss: String,
    aud: String,
    sender: Identity,
    iat: u64,
    exp: u64,
}

/// Mint credentials from trusted host state, not a caller-supplied identity.
pub fn sign(key: &EncodingKey, sender: Identity, receiver: Identity) -> anyhow::Result<String> {
    let iat = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let claims = Claims {
        iss: ISSUER.into(),
        aud: receiver.to_hex().to_string(),
        sender,
        iat,
        exp: iat + TOKEN_LIFETIME_SECS,
    };
    Ok(jsonwebtoken::encode(&Header::new(Algorithm::ES256), &claims, key)?)
}

/// Authenticate only with an explicitly trusted host key, never OIDC discovery.
pub fn verify(key: &DecodingKey, token: &str, receiver: Identity) -> anyhow::Result<Identity> {
    let mut validation = Validation::new(Algorithm::ES256);
    validation.set_required_spec_claims(&["iss", "aud", "exp", "iat"]);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[receiver.to_hex().to_string()]);
    validation.leeway = 0;
    let claims = jsonwebtoken::decode::<Claims>(token, key, &validation)?.claims;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    anyhow::ensure!(claims.iat <= now && claims.exp > now, "invalid IDC token timestamps");
    anyhow::ensure!(
        claims.exp > claims.iat && claims.exp - claims.iat <= TOKEN_LIFETIME_SECS,
        "invalid IDC token lifetime"
    );
    Ok(claims.sender)
}
