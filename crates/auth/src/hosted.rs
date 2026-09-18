//! Target-bound credentials for a database's hosted container.
//!
//! Decoded claims are untrusted. Only signature verification against an explicitly
//! configured platform issuer and comparison with an authoritative instance/grant
//! binding can produce [`VerifiedHostedAuth`]. Receiving hosts must additionally
//! recheck their durable target fence at each transaction and subscription admission.

use crate::identity::{ConnectionAuthCtx, SpacetimeIdentityClaims};
use anyhow::{bail, ensure, Context};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use spacetimedb_lib::Identity;
use std::fmt;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const HOSTED_TOKEN_KIND: &str = "spacetimedb_hosted_v1";
pub const HOSTED_TOKEN_TYPE: &str = "spacetimedb-hosted+jwt";
pub const MAX_HOSTED_TOKEN_LIFETIME: Duration = Duration::from_secs(30);
pub const MAX_HOSTED_TOKEN_BYTES: usize = 8192;

/// Wire claims, deliberately distinct from ordinary issuer/subject-derived Identity claims.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostedTokenClaims {
    pub kind: Box<str>,
    #[serde(rename = "iss")]
    pub issuer: Box<str>,
    #[serde(rename = "sub")]
    pub subject: Box<str>,
    #[serde(with = "identity_hex")]
    pub source_database: Identity,
    /// A scalar, canonical database Identity. Lists and database names are not accepted.
    #[serde(rename = "aud", with = "identity_hex")]
    pub target_database: Identity,
    pub generation: u64,
    pub grant_revision: u64,
    #[serde(rename = "iat")]
    pub issued_at: u64,
    #[serde(rename = "exp")]
    pub expires_at: u64,
    #[serde(rename = "jti")]
    pub token_id: Box<str>,
}

/// Trusted input obtained from current source registration, placement and target grant state.
/// Never construct this by copying the incoming token's claims. Admission must be open,
/// and the assigned node/incarnation and required module capability must already be checked.
#[derive(Clone, Copy, Debug)]
pub struct HostedTokenBinding {
    pub source_database: Identity,
    pub target_database: Identity,
    pub generation: u64,
    pub grant_revision: u64,
    pub lease_expires_at: SystemTime,
}

/// Authentication proof. It cannot be deserialized or constructed from decoded claims.
#[derive(Clone)]
pub struct VerifiedHostedAuth {
    claims: HostedTokenClaims,
    // Local lifetime state only. Never serialized into signed claims.
    monotonic_deadline: Instant,
}

impl fmt::Debug for VerifiedHostedAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerifiedHostedAuth")
            .field("source_database", &self.source_database())
            .field("target_database", &self.target_database())
            .field("generation", &self.generation())
            .field("grant_revision", &self.grant_revision())
            .field("expires_at", &self.expires_at())
            .finish_non_exhaustive()
    }
}

impl VerifiedHostedAuth {
    pub fn source_database(&self) -> Identity {
        self.claims.source_database
    }
    pub fn target_database(&self) -> Identity {
        self.claims.target_database
    }
    pub fn generation(&self) -> u64 {
        self.claims.generation
    }
    pub fn grant_revision(&self) -> u64 {
        self.claims.grant_revision
    }
    pub fn issued_at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.claims.issued_at)
    }
    pub fn expires_at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.claims.expires_at)
    }
    pub fn token_id(&self) -> &str {
        &self.claims.token_id
    }
    pub fn issuer(&self) -> &str {
        &self.claims.issuer
    }
    pub fn is_internal(&self) -> bool {
        self.source_database() == self.target_database()
    }

    /// Expiry has no positive leeway. This does not replace generation/grant fencing.
    pub fn check_at(&self, now: SystemTime) -> anyhow::Result<()> {
        self.check_at_clocks(now, Instant::now())
    }

    fn check_at_clocks(&self, now: SystemTime, monotonic_now: Instant) -> anyhow::Result<()> {
        ensure!(now >= self.issued_at(), "hosted credential is not yet valid");
        ensure!(
            now < self.expires_at() && monotonic_now < self.monotonic_deadline,
            "hosted credential expired"
        );
        Ok(())
    }

    /// Cap the local lifetime using a fresh authority confirmation. The caller
    /// records `confirmation_started` before sending the request and obtains
    /// `confirmed_time` from the successful authoritative response. Charging the
    /// entire round trip against the signed remaining lifetime is conservative.
    /// Neither a receiving clock behind authority nor a later confirmation can
    /// extend this proof. The original signed claims are preserved exactly.
    pub fn constrain_expiration(
        mut self,
        confirmed_time: SystemTime,
        confirmation_started: Instant,
    ) -> anyhow::Result<Self> {
        let now = Instant::now();
        ensure!(confirmation_started <= now, "invalid hosted confirmation clock");
        self.check_at_clocks(confirmed_time, now)?;
        let remaining = self.expires_at().duration_since(confirmed_time)?;
        let confirmed_deadline = confirmation_started
            .checked_add(remaining)
            .context("hosted confirmation deadline overflow")?;
        self.monotonic_deadline = self.monotonic_deadline.min(confirmed_deadline);
        self.check_at_clocks(confirmed_time, now)?;
        Ok(self)
    }

    /// Remaining connection lifetime, bounded by both signed wall-clock expiry
    /// and the already captured local deadline. Repeated calls never reset it.
    pub fn remaining_lifetime(&self, now: SystemTime) -> Duration {
        self.remaining_lifetime_at_clocks(now, Instant::now())
    }

    fn remaining_lifetime_at_clocks(&self, now: SystemTime, monotonic_now: Instant) -> Duration {
        if self.check_at_clocks(now, monotonic_now).is_err() {
            return Duration::ZERO;
        }
        self.expires_at()
            .duration_since(now)
            .unwrap_or_default()
            .min(self.monotonic_deadline.saturating_duration_since(monotonic_now))
    }

    pub fn into_connection_auth(self) -> anyhow::Result<ConnectionAuthCtx> {
        // Keep the actual claims, including the source/target/generation restrictions.
        // Normalizing JSON whitespace does not alter any signed claim values.
        let jwt_payload = serde_json::to_string(&self.claims)?.into_boxed_str();
        let mut extra = serde_json::to_value(&self.claims)?;
        let extra = extra.as_object_mut().expect("hosted claims serialize as an object");
        for key in ["iss", "sub", "aud", "iat", "exp"] {
            extra.remove(key);
        }
        let claims = SpacetimeIdentityClaims {
            identity: self.source_database(),
            subject: self.claims.subject.clone(),
            issuer: self.claims.issuer.clone(),
            audience: [self.target_database().to_hex().to_string().into_boxed_str()].into(),
            iat: self.issued_at(),
            exp: Some(self.expires_at()),
            extra: Some(
                extra
                    .iter()
                    .map(|(key, value)| (key.clone().into_boxed_str(), value.clone()))
                    .collect(),
            ),
        };
        Ok(ConnectionAuthCtx {
            claims,
            jwt_payload,
            hosted: Some(self),
        })
    }
}

/// Classifies the reserved namespace only. A positive result grants no authority.
/// All reserved versions are rejected by ordinary OIDC validation and token exchange.
pub fn has_reserved_hosted_token_kind(token: &str) -> anyhow::Result<bool> {
    classify_reserved_token(token, is_reserved_hosted_type, is_reserved_hosted_kind)
}

/// Operational container proofs are never client Identity credentials. Reserve
/// their entire versioned namespace so a lease/registry proof cannot enter
/// OIDC discovery, ordinary JWT validation, or the Identity token exchange.
pub fn has_reserved_platform_token_kind(token: &str) -> anyhow::Result<bool> {
    classify_reserved_token(
        token,
        |kind| is_reserved_hosted_type(kind) || kind.starts_with("spacetimedb-container-"),
        |kind| is_reserved_hosted_kind(kind) || kind.starts_with("spacetimedb_container_"),
    )
}

fn classify_reserved_token(
    token: &str,
    reserved_type: impl FnOnce(&str) -> bool,
    reserved_kind: impl FnOnce(&str) -> bool,
) -> anyhow::Result<bool> {
    let header = decode_header(token)?;
    if header.typ.as_deref().is_some_and(reserved_type) {
        return Ok(true);
    }
    let data = jsonwebtoken::dangerous::insecure_decode::<serde_json::Value>(token)?;
    Ok(data
        .claims
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .is_some_and(reserved_kind))
}

pub fn is_reserved_hosted_kind(kind: &str) -> bool {
    kind.starts_with("spacetimedb_hosted_")
}
fn is_reserved_hosted_type(kind: &str) -> bool {
    kind.starts_with("spacetimedb-hosted")
}

/// Decode routing hints only, never authentication. The caller must use these hints to
/// find trusted registration/grant state and then call [`verify_hosted_token`].
pub fn unverified_hosted_token_claims(token: &str) -> anyhow::Result<HostedTokenClaims> {
    ensure!(token.len() <= MAX_HOSTED_TOKEN_BYTES, "hosted credential too large");
    Ok(jsonwebtoken::dangerous::insecure_decode::<HostedTokenClaims>(token)?.claims)
}

/// Verify against a configured key and issuer, never a JWT-supplied key or JWKS URL.
/// `binding` must be authoritative state for that issuer's registered source database.
pub fn verify_hosted_token(
    token: &str,
    public_key: &DecodingKey,
    trusted_issuer: &str,
    binding: &HostedTokenBinding,
    now: SystemTime,
) -> anyhow::Result<VerifiedHostedAuth> {
    let verification_started = Instant::now();
    ensure!(token.len() <= MAX_HOSTED_TOKEN_BYTES, "hosted credential too large");
    let header = decode_header(token)?;
    ensure!(header.alg == Algorithm::ES256, "hosted credential requires ES256");
    ensure!(
        header.typ.as_deref() == Some(HOSTED_TOKEN_TYPE),
        "invalid hosted credential type"
    );
    let mut validation = Validation::new(Algorithm::ES256);
    validation.set_required_spec_claims(&["iss", "sub", "aud", "exp"]);
    validation.set_issuer(&[trusted_issuer]);
    validation.set_audience(&[binding.target_database.to_hex().to_string()]);
    validation.leeway = 0;
    // Check time below against the caller's trusted clock, including exact expiry equality.
    validation.validate_exp = false;
    let claims = decode::<HostedTokenClaims>(token, public_key, &validation)?.claims;
    validate_claims(&claims, trusted_issuer, binding, now)?;
    let remaining = (UNIX_EPOCH + Duration::from_secs(claims.expires_at)).duration_since(now)?;
    let monotonic_deadline = verification_started
        .checked_add(remaining)
        .context("hosted credential deadline overflow")?;
    let proof = VerifiedHostedAuth {
        claims,
        monotonic_deadline,
    };
    proof.check_at(now)?;
    Ok(proof)
}

/// Mint from the broker's authoritative binding, with no guest-selected sender or generation.
pub fn sign_hosted_token(
    private_key: &EncodingKey,
    trusted_issuer: &str,
    binding: &HostedTokenBinding,
    now: SystemTime,
    expires_at: SystemTime,
    token_id: &str,
) -> anyhow::Result<String> {
    let claims = HostedTokenClaims {
        kind: HOSTED_TOKEN_KIND.into(),
        issuer: trusted_issuer.into(),
        subject: binding.source_database.to_hex().to_string().into_boxed_str(),
        source_database: binding.source_database,
        target_database: binding.target_database,
        generation: binding.generation,
        grant_revision: binding.grant_revision,
        issued_at: unix_seconds(now)?,
        expires_at: unix_seconds(expires_at)?,
        token_id: token_id.into(),
    };
    validate_claims(&claims, trusted_issuer, binding, now)?;
    let mut header = Header::new(Algorithm::ES256);
    header.typ = Some(HOSTED_TOKEN_TYPE.into());
    Ok(jsonwebtoken::encode(&header, &claims, private_key)?)
}

fn validate_claims(
    claims: &HostedTokenClaims,
    issuer: &str,
    binding: &HostedTokenBinding,
    now: SystemTime,
) -> anyhow::Result<()> {
    ensure!(
        !issuer.is_empty() && issuer.len() <= 128,
        "invalid trusted hosted issuer"
    );
    ensure!(
        claims.kind.as_ref() == HOSTED_TOKEN_KIND,
        "unsupported hosted credential kind"
    );
    ensure!(claims.issuer.as_ref() == issuer, "untrusted hosted credential issuer");
    ensure!(
        claims.source_database == binding.source_database,
        "hosted credential source mismatch"
    );
    ensure!(
        claims.target_database == binding.target_database,
        "hosted credential target mismatch"
    );
    ensure!(
        claims.subject.as_ref() == claims.source_database.to_hex().as_str(),
        "hosted credential subject mismatch"
    );
    ensure!(
        claims.generation == binding.generation,
        "hosted credential generation mismatch"
    );
    ensure!(
        claims.grant_revision == binding.grant_revision,
        "hosted credential grant revision mismatch"
    );
    ensure!(
        !claims.token_id.is_empty()
            && claims.token_id.len() <= 128
            && claims
                .token_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid hosted credential token ID"
    );
    let Some(lifetime) = claims.expires_at.checked_sub(claims.issued_at) else {
        bail!("invalid hosted credential lifetime")
    };
    ensure!(
        lifetime > 0 && lifetime <= MAX_HOSTED_TOKEN_LIFETIME.as_secs(),
        "hosted credential lifetime exceeds limit"
    );
    let issued_at = UNIX_EPOCH
        .checked_add(Duration::from_secs(claims.issued_at))
        .context("invalid hosted issue time")?;
    let expires_at = UNIX_EPOCH
        .checked_add(Duration::from_secs(claims.expires_at))
        .context("invalid hosted expiry")?;
    ensure!(
        issued_at <= now && now < expires_at,
        "hosted credential outside validity interval"
    );
    ensure!(
        expires_at <= binding.lease_expires_at,
        "hosted credential exceeds confirmed lease"
    );
    Ok(())
}

fn unix_seconds(time: SystemTime) -> anyhow::Result<u64> {
    Ok(time.duration_since(UNIX_EPOCH)?.as_secs())
}

mod identity_hex {
    use super::*;
    pub fn serialize<S: serde::Serializer>(value: &Identity, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(value.to_hex().as_str())
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Identity, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
            return Err(serde::de::Error::custom(
                "expected canonical 64-character lowercase database Identity",
            ));
        }
        Identity::from_hex(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "hosted/expiration_tests.rs"]
mod expiration_tests;
