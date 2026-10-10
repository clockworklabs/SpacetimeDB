use jsonwebtoken::{DecodingKey, EncodingKey};
use openssl::ec::{EcGroup, EcKey};
use openssl::nid::Nid;
use openssl::pkey::PKey;
use spacetimedb_paths::cli::{PrivKeyPath, PubKeyPath};

use crate::config::CertificateAuthority;

pub use spacetimedb_auth::{idc, identity};
pub mod token_validation;

#[cfg(test)]
mod idc_tests {
    use super::*;
    use serde_json::json;
    use spacetimedb_lib::Identity;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn idc_credentials_require_trusted_key_recipient_and_expiry() -> anyhow::Result<()> {
        let keys = JwtKeys::generate()?;
        let sender = Identity::ZERO;
        let receiver = Identity::ONE;
        let token = idc::sign(&keys.private, sender, receiver)?;
        assert_eq!(idc::verify(&keys.public, &token, receiver)?, sender);
        assert!(idc::verify(&keys.public, &token, sender).is_err());
        assert!(idc::verify(&JwtKeys::generate()?.public, &token, receiver).is_err());

        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let claims = json!({
            "iss": "spacetimedb-idc-v1", "aud": receiver.to_hex().to_string(),
            "sender": sender, "iat": now, "exp": now + 60,
        });
        for (field, value) in [
            ("iss", json!("client-issuer")),
            ("exp", json!(now - 1)),
            ("exp", json!(now + 3600)),
            ("iat", json!(now + 30)),
            ("sender", json!("invalid")),
        ] {
            let mut invalid = claims.clone();
            invalid[field] = value;
            let token = jsonwebtoken::encode(
                &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256),
                &invalid,
                &keys.private,
            )?;
            assert!(
                idc::verify(&keys.public, &token, receiver).is_err(),
                "accepted invalid {field}"
            );
        }
        for field in ["iss", "aud", "sender", "iat", "exp"] {
            let mut invalid = claims.clone();
            invalid.as_object_mut().unwrap().remove(field);
            let token = jsonwebtoken::encode(
                &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256),
                &invalid,
                &keys.private,
            )?;
            assert!(
                idc::verify(&keys.public, &token, receiver).is_err(),
                "accepted missing {field}"
            );
        }
        let forged = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(&keys.public_pem),
        )?;
        assert!(idc::verify(&keys.public, &forged, receiver).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn idc_credentials_are_not_client_tokens() -> anyhow::Result<()> {
        use token_validation::TokenValidator;
        let keys = JwtKeys::generate()?;
        let token = idc::sign(&keys.private, Identity::ZERO, Identity::ONE)?;
        assert!(keys.public.validate_token(&token).await.is_err());
        Ok(())
    }
}

/// JWT verification and signing keys.
#[derive(Clone)]
pub struct JwtKeys {
    pub public: DecodingKey,
    pub public_pem: Box<[u8]>,
    pub private: EncodingKey,
    pub private_pem: Box<[u8]>,
    pub kid: Option<String>,
}

impl JwtKeys {
    /// Create a new [`JwtKeys`] from paths to the public and private key files
    /// respectively.
    ///
    /// The key files must be PEM encoded ECDSA P256 keys.
    pub fn new(public_pem: impl Into<Box<[u8]>>, private_pem: impl Into<Box<[u8]>>) -> anyhow::Result<Self> {
        let public_pem = public_pem.into();
        let private_pem = private_pem.into();
        let public = DecodingKey::from_ec_pem(&public_pem)?;
        let private = EncodingKey::from_ec_pem(&private_pem)?;

        Ok(Self {
            public,
            private,
            public_pem,
            private_pem,
            kid: None,
        })
    }

    pub fn generate() -> anyhow::Result<Self> {
        let keypair = EcKeyPair::generate()?;
        keypair.try_into()
    }
}

// Get the key pair if the given files exist. If they don't, create them.
// If only one of the files exists, return an error.
pub fn get_or_create_keys(certs: &CertificateAuthority) -> anyhow::Result<JwtKeys> {
    let public_key_path = &certs.jwt_pub_key_path;
    let private_key_path = &certs.jwt_priv_key_path;

    let public_key_bytes = public_key_path.read().ok();
    let private_key_bytes = private_key_path.read().ok();

    // If both keys are unspecified, create them
    let key_pair = match (public_key_bytes, private_key_bytes) {
        (Some(pub_), Some(priv_)) => EcKeyPair::new(pub_, priv_),
        (None, None) => {
            let keys = EcKeyPair::generate()?;
            keys.write_to_files(public_key_path, private_key_path)?;
            keys
        }
        (None, Some(_)) => anyhow::bail!("Unable to read public key for JWT token verification"),
        (Some(_), None) => anyhow::bail!("Unable to read private key for JWT token signing"),
    };

    key_pair.try_into()
}

// An Ec key pair in pem format.
pub struct EcKeyPair {
    pub public_key_bytes: Vec<u8>,
    pub private_key_bytes: Vec<u8>,
}

impl TryFrom<EcKeyPair> for JwtKeys {
    type Error = anyhow::Error;
    fn try_from(pair: EcKeyPair) -> anyhow::Result<Self> {
        JwtKeys::new(pair.public_key_bytes, pair.private_key_bytes)
    }
}

impl EcKeyPair {
    pub fn new(public_key_bytes: Vec<u8>, private_key_bytes: Vec<u8>) -> Self {
        Self {
            public_key_bytes,
            private_key_bytes,
        }
    }

    pub fn generate() -> anyhow::Result<Self> {
        // Create a new EC group from a named curve.
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;

        // Create a new EC key with the specified group.
        let eckey = EcKey::generate(&group)?;

        // Create a new PKey from the EC key.
        let pkey = PKey::from_ec_key(eckey.clone())?;

        // Get the private key in PKCS#8 PEM format & write it.
        let private_key_bytes = pkey.private_key_to_pem_pkcs8()?;

        // Get the public key in PEM format & write it.
        let public_key_bytes = eckey.public_key_to_pem()?;

        Ok(Self {
            public_key_bytes,
            private_key_bytes,
        })
    }

    pub fn write_to_files(&self, public_key_path: &PubKeyPath, private_key_path: &PrivKeyPath) -> anyhow::Result<()> {
        public_key_path.write(&self.public_key_bytes)?;
        private_key_path.write(&self.private_key_bytes)?;
        Ok(())
    }
}
