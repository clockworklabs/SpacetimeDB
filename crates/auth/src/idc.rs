//! Host-signed transport credentials. These are never exposed as module JWTs.

use crate::token_signing::TokenSigner;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use spacetimedb_lib::Identity;
use std::time::{SystemTime, UNIX_EPOCH};

const ISSUER: &str = "spacetimedb-idc-v1";
// Keep leaked credentials short-lived. The actor signs a fresh token for each
// delivery attempt, including retries, so queued messages do not need long-lived tokens.
// TODO(idc): Consider caching tokens per sender/receiver pair and extending their lifetime
// to reduce signing overhead, refreshing them before expiry.
const TOKEN_LIFETIME_SECS: u64 = 60;

/// Signs transport credentials for calls between databases.
pub trait IdcTokenSigner: Send + Sync {
    /// Attest the sending database's identity to the receiver.
    fn sign(&self, sender: Identity, receiver: Identity) -> anyhow::Result<String>;
}

impl<T: TokenSigner> IdcTokenSigner for T {
    fn sign(&self, sender: Identity, receiver: Identity) -> anyhow::Result<String> {
        sign(self, sender, receiver)
    }
}

/// We can't use [`crate::identity::IncomingClaims`] here, client auth derives
/// identity from `(iss, sub)`, and we don't store that pair for databases.
/// Using the database identity as a new subject would give us a different identity.
/// Instead, a trusted host signs the database's existing identity directly.
/// The token is only used between hosts, not exposed through `sender_auth().jwt()`.
#[derive(Serialize, Deserialize)]
struct Claims {
    iss: String,
    aud: String,
    sender: Identity,
    iat: u64,
    exp: u64,
}

/// Mint credentials from trusted host state, not a caller-supplied identity.
pub fn sign(signer: &impl TokenSigner, sender: Identity, receiver: Identity) -> anyhow::Result<String> {
    let iat = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let claims = Claims {
        iss: ISSUER.into(),
        aud: receiver.to_hex().to_string(),
        sender,
        iat,
        exp: iat + TOKEN_LIFETIME_SECS,
    };
    Ok(signer.sign(&claims)?)
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
