//! Explicit platform trust for hosted database credentials. No OIDC discovery or fallback.

use anyhow::{ensure, Context};
use jsonwebtoken::DecodingKey;
pub use spacetimedb_auth::hosted::{
    has_reserved_hosted_token_kind, sign_hosted_token, HostedTokenBinding, HostedTokenClaims, VerifiedHostedAuth, HOSTED_TOKEN_KIND, HOSTED_TOKEN_TYPE,
    MAX_HOSTED_TOKEN_LIFETIME,
};
use spacetimedb_auth::hosted::{unverified_hosted_token_claims, verify_hosted_token};
use spacetimedb_lib::Identity;
use std::collections::HashMap;
use std::time::SystemTime;

/// Only configured platform signers can attest registered source databases.
pub struct HostedTokenValidator {
    trusted_issuers: HashMap<Box<str>, DecodingKey>,
}

impl HostedTokenValidator {
    pub fn new(issuers: impl IntoIterator<Item = (Box<str>, DecodingKey)>) -> anyhow::Result<Self> {
        let mut trusted_issuers = HashMap::new();
        for (issuer, key) in issuers {
            ensure!(
                !issuer.is_empty() && issuer.len() <= 128,
                "invalid trusted hosted issuer"
            );
            ensure!(
                trusted_issuers.insert(issuer, key).is_none(),
                "duplicate trusted hosted issuer"
            );
        }
        Ok(Self { trusted_issuers })
    }

    /// `resolve_binding` reads authoritative state, including this issuer's source
    /// registration, open admission, current placement/incarnation and target grant.
    /// Return None if any requirement is absent. Its inputs are untrusted routing hints;
    /// the callback must never copy them into a fabricated binding or mutate state.
    /// The returned proof still requires target-fence checks at every later admission.
    pub fn validate_token(
        &self,
        token: &str,
        target: Identity,
        now: SystemTime,
        resolve_binding: impl FnOnce(&str, Identity, Identity) -> Option<HostedTokenBinding>,
    ) -> anyhow::Result<VerifiedHostedAuth> {
        let hints = unverified_hosted_token_claims(token)?;
        ensure!(hints.target_database == target, "hosted credential target mismatch");
        let key = self
            .trusted_issuers
            .get(&hints.issuer)
            .context("untrusted hosted credential issuer")?;
        let binding = resolve_binding(&hints.issuer, hints.source_database, target)
            .context("hosted source registration, instance, or target grant is unavailable")?;
        ensure!(
            binding.target_database == target,
            "authoritative hosted binding target mismatch"
        );
        verify_hosted_token(token, key, &hints.issuer, &binding, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{
        token_validation::{FullTokenValidator, TokenValidator, UnimplementedTokenValidator},
        JwtKeys,
    };
    use jsonwebtoken::{Algorithm, Header};
    use serde_json::{json, Value};
    use spacetimedb_auth::hosted::{has_reserved_hosted_token_kind, MAX_HOSTED_TOKEN_BYTES};
    use std::time::{Duration, UNIX_EPOCH};

    fn fixture() -> (JwtKeys, HostedTokenValidator, HostedTokenBinding, SystemTime) {
        let keys = JwtKeys::generate().unwrap();
        let validator = HostedTokenValidator::new([("platform.test".into(), keys.public.clone())]).unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let binding = HostedTokenBinding {
            source_database: Identity::from_claims("source", "database"),
            target_database: Identity::from_claims("target", "database"),
            generation: 9_007_199_254_740_993,
            grant_revision: 9_007_199_254_740_995,
            lease_expires_at: now + Duration::from_secs(30),
        };
        (keys, validator, binding, now)
    }

    fn mint(keys: &JwtKeys, binding: &HostedTokenBinding, now: SystemTime) -> String {
        sign_hosted_token(
            &keys.private,
            "platform.test",
            binding,
            now,
            now + Duration::from_secs(20),
            "token-private-id",
        )
        .unwrap()
    }

    fn change(token: &str, keys: &JwtKeys, mutate: impl FnOnce(&mut Value, &mut Header)) -> String {
        let mut claims = serde_json::to_value(unverified_hosted_token_claims(token).unwrap()).unwrap();
        let mut header = Header::new(Algorithm::ES256);
        header.typ = Some(HOSTED_TOKEN_TYPE.into());
        mutate(&mut claims, &mut header);
        jsonwebtoken::encode(&header, &claims, &keys.private).unwrap()
    }

    #[test]
    fn hosted_sender_target_authority_and_claims_are_preserved() {
        let (keys, validator, mut binding, now) = fixture();
        for self_call in [false, true] {
            if self_call {
                binding.target_database = binding.source_database;
            }
            let token = mint(&keys, &binding, now);
            assert!(has_reserved_hosted_token_kind(&token).unwrap());
            let verified = validator
                .validate_token(&token, binding.target_database, now, |issuer, source, target| {
                    assert_eq!(issuer, "platform.test");
                    assert_eq!(source, binding.source_database);
                    assert_eq!(target, binding.target_database);
                    Some(binding)
                })
                .unwrap();
            assert_eq!(verified.is_internal(), self_call);
            assert_eq!(verified.generation(), binding.generation);
            assert_eq!(verified.grant_revision(), binding.grant_revision);
            assert!(verified.check_at(now + Duration::from_secs(20)).is_err());
            let ctx = verified.into_connection_auth().unwrap();
            assert_eq!(ctx.claims.identity, binding.source_database);
            assert_ne!(
                ctx.claims.identity,
                Identity::from_claims(&ctx.claims.issuer, &ctx.claims.subject)
            );
            assert!(ctx.hosted.is_some());
            let payload: Value = serde_json::from_str(&ctx.jwt_payload).unwrap();
            assert_eq!(payload["generation"].as_u64(), Some(binding.generation));
            assert_eq!(payload["grant_revision"].as_u64(), Some(binding.grant_revision));
            assert_eq!(payload["aud"], binding.target_database.to_hex().as_str());
            assert_eq!(payload["iss"], "platform.test");
            let debug = format!("{ctx:?}");
            assert!(!debug.contains("token-private-id"));
            assert!(!debug.contains(&token));
            assert!(!debug.contains("jwt_payload"));
        }
    }

    #[test]
    fn hosted_validation_rejects_wrong_authority_binding_and_wire_shape() {
        let (keys, validator, binding, now) = fixture();
        let token = mint(&keys, &binding, now);
        let other_keys = JwtKeys::generate().unwrap();
        assert!(validator
            .validate_token(
                &mint(&other_keys, &binding, now),
                binding.target_database,
                now,
                |_, _, _| Some(binding)
            )
            .is_err());
        assert!(validator
            .validate_token(&token, binding.source_database, now, |_, _, _| Some(binding))
            .is_err());
        assert!(validator
            .validate_token(&token, binding.target_database, now, |_, _, _| None)
            .is_err());
        let invalid_fields = [
            ("kind", json!("spacetimedb_hosted_v2")),
            ("iss", json!("unknown.test")),
            ("source_database", json!(binding.target_database.to_hex().as_str())),
            ("sub", json!("other")),
            ("aud", json!(binding.source_database.to_hex().as_str())),
            ("aud", json!([binding.target_database.to_hex().as_str()])),
            ("generation", json!(binding.generation - 1)),
            ("grant_revision", json!(binding.grant_revision - 1)),
            ("iat", json!(1_700_000_001_u64)),
            ("exp", json!(1_700_000_000_u64)),
            ("exp", json!(1_700_000_031_u64)),
            ("exp", json!(u64::MAX)),
            ("jti", json!("")),
            ("hex_identity", json!(binding.source_database.to_hex().as_str())),
        ];
        for (field, value) in invalid_fields {
            let changed = change(&token, &keys, |claims, _| claims[field] = value);
            assert!(
                validator
                    .validate_token(&changed, binding.target_database, now, |_, _, _| Some(binding))
                    .is_err(),
                "accepted changed {field}"
            );
        }
        let wrong_type = change(&token, &keys, |_, header| header.typ = Some("JWT".into()));
        assert!(validator
            .validate_token(&wrong_type, binding.target_database, now, |_, _, _| Some(binding))
            .is_err());
        let missing_exp = change(&token, &keys, |claims, _| {
            claims.as_object_mut().unwrap().remove("exp");
        });
        assert!(validator
            .validate_token(&missing_exp, binding.target_database, now, |_, _, _| Some(binding))
            .is_err());
        let overflowing_time = change(&token, &keys, |claims, _| {
            claims["iat"] = json!(u64::MAX - 20);
            claims["exp"] = json!(u64::MAX);
        });
        assert!(validator
            .validate_token(&overflowing_time, binding.target_database, now, |_, _, _| Some(binding))
            .is_err());
        let short_lease = HostedTokenBinding {
            lease_expires_at: now + Duration::from_secs(19),
            ..binding
        };
        assert!(validator
            .validate_token(&token, binding.target_database, now, |_, _, _| Some(short_lease))
            .is_err());
        assert!(validator
            .validate_token(
                &"x".repeat(MAX_HOSTED_TOKEN_BYTES + 1),
                binding.target_database,
                now,
                |_, _, _| Some(binding)
            )
            .is_err());
        let claims = unverified_hosted_token_claims(&token).unwrap();
        let mut hs_header = Header::new(Algorithm::HS256);
        hs_header.typ = Some(HOSTED_TOKEN_TYPE.into());
        let hs_token = jsonwebtoken::encode(
            &hs_header,
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(b"not-a-platform-key"),
        )
        .unwrap();
        assert!(validator
            .validate_token(&hs_token, binding.target_database, now, |_, _, _| Some(binding))
            .is_err());
    }

    #[test]
    fn broker_signing_obeys_confirmed_lease_and_lifetime() {
        let (keys, _, binding, now) = fixture();
        for expiry in [now, now + Duration::from_secs(31)] {
            assert!(sign_hosted_token(&keys.private, "platform.test", &binding, now, expiry, "id").is_err());
        }
        let short_lease = HostedTokenBinding {
            lease_expires_at: now + Duration::from_secs(10),
            ..binding
        };
        assert!(sign_hosted_token(
            &keys.private,
            "platform.test",
            &short_lease,
            now,
            now + Duration::from_secs(11),
            "id"
        )
        .is_err());
    }

    #[tokio::test]
    async fn ordinary_validation_rejects_reserved_hosted_kinds_and_types() {
        let (keys, _, binding, _) = fixture();
        let now = SystemTime::now();
        let binding = HostedTokenBinding {
            lease_expires_at: now + Duration::from_secs(30),
            ..binding
        };
        let token = mint(&keys, &binding, now);
        let ordinary = FullTokenValidator {
            local_key: keys.public.clone(),
            local_issuer: "platform.test".into(),
            oidc_validator: UnimplementedTokenValidator,
        };
        for reserved in [
            token.clone(),
            change(&token, &keys, |claims, header| {
                claims["kind"] = json!("spacetimedb_hosted_future");
                header.typ = Some("JWT".into());
            }),
            change(&token, &keys, |claims, header| {
                claims.as_object_mut().unwrap().remove("kind");
                header.typ = Some("spacetimedb-hosted-v2+jwt".into());
            }),
        ] {
            assert!(has_reserved_hosted_token_kind(&reserved).unwrap());
            assert!(keys.public.validate_token(&reserved).await.is_err());
            assert!(ordinary.validate_token(&reserved).await.is_err());
        }
    }

    #[tokio::test]
    async fn reserved_classification_preserves_ordinary_token_algorithms() {
        let rsa = openssl::rsa::Rsa::generate(2048).unwrap();
        let rsa = openssl::pkey::PKey::from_rsa(rsa).unwrap();
        let ec = JwtKeys::generate().unwrap();
        let keys = [
            (Algorithm::ES256, ec.private, ec.public),
            (
                Algorithm::RS256,
                jsonwebtoken::EncodingKey::from_rsa_pem(&rsa.private_key_to_pem_pkcs8().unwrap()).unwrap(),
                DecodingKey::from_rsa_pem(&rsa.public_key_to_pem().unwrap()).unwrap(),
            ),
            (
                Algorithm::HS256,
                jsonwebtoken::EncodingKey::from_secret(b"ordinary-oidc-test-secret"),
                DecodingKey::from_secret(b"ordinary-oidc-test-secret"),
            ),
        ];
        for (algorithm, private, public) in keys {
            let claims = json!({ "iss": "ordinary.test", "sub": "a-user", "iat": 1_700_000_000_u64, "kind": "ordinary_application_kind" });
            let token = jsonwebtoken::encode(&Header::new(algorithm), &claims, &private).unwrap();
            assert!(
                !has_reserved_hosted_token_kind(&token).unwrap(),
                "misclassified {algorithm:?}"
            );
            let validated = public.validate_token(&token).await.unwrap();
            assert_eq!(validated.identity, Identity::from_claims("ordinary.test", "a-user"));
        }
    }
}
