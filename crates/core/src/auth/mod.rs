use jsonwebtoken::{DecodingKey, EncodingKey};
use rcgen::KeyPair;
use spacetimedb_paths::cli::{PrivKeyPath, PubKeyPath};

use crate::config::CertificateAuthority;

pub use spacetimedb_auth::identity;
pub mod token_validation;

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
        // Generate a new key pair for the P-256 curve (equivalent to `Nid::X9_62_PRIME256V1`).
        let key_pair = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;

        // Get the public key in PEM format.
        let public_key_bytes = key_pair.public_key_pem().into_bytes();

        // Get the private key in PKCS#8 PEM format.
        let private_key_bytes = key_pair.serialize_pem().into_bytes();

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

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{Algorithm, Header, Validation};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestClaims {
        sub: String,
        exp: u64,
    }

    #[test]
    fn generate_produces_valid_pem_headers() {
        let pair = EcKeyPair::generate().expect("key generation should succeed");

        let pub_str = std::str::from_utf8(&pair.public_key_bytes).expect("public key should be valid UTF-8");
        let priv_str = std::str::from_utf8(&pair.private_key_bytes).expect("private key should be valid UTF-8");

        assert!(
            pub_str.contains("-----BEGIN PUBLIC KEY-----"),
            "public key should be SPKI PEM format, got: {pub_str}"
        );
        assert!(
            priv_str.contains("-----BEGIN PRIVATE KEY-----"),
            "private key should be PKCS#8 PEM format, got: {priv_str}"
        );
    }

    #[test]
    fn generate_roundtrip_sign_verify() {
        let pair = EcKeyPair::generate().expect("key generation should succeed");
        let jwt_keys = JwtKeys::try_from(pair).expect("JwtKeys conversion should succeed");

        let claims = TestClaims {
            sub: "test-user".to_string(),
            exp: u64::MAX,
        };

        let token = jsonwebtoken::encode(&Header::new(Algorithm::ES256), &claims, &jwt_keys.private)
            .expect("JWT signing should succeed");

        let mut validation = Validation::new(Algorithm::ES256);
        validation.required_spec_claims.clear();
        let decoded = jsonwebtoken::decode::<TestClaims>(&token, &jwt_keys.public, &validation)
            .expect("JWT verification should succeed");

        assert_eq!(decoded.claims, claims);
    }

    #[test]
    fn generate_produces_unique_keys() {
        let pair1 = EcKeyPair::generate().expect("first key generation should succeed");
        let pair2 = EcKeyPair::generate().expect("second key generation should succeed");

        assert_ne!(
            pair1.private_key_bytes, pair2.private_key_bytes,
            "two generated key pairs should have different private keys"
        );
    }

    #[test]
    fn generated_keys_cross_verify_fails() {
        let pair1 = EcKeyPair::generate().expect("first key generation should succeed");
        let pair2 = EcKeyPair::generate().expect("second key generation should succeed");
        let keys1 = JwtKeys::try_from(pair1).unwrap();
        let keys2 = JwtKeys::try_from(pair2).unwrap();

        let claims = TestClaims {
            sub: "test".to_string(),
            exp: u64::MAX,
        };

        // Sign with key1's private key.
        let token = jsonwebtoken::encode(&Header::new(Algorithm::ES256), &claims, &keys1.private).unwrap();

        // Verify with key2's public key should fail.
        let mut validation = Validation::new(Algorithm::ES256);
        validation.required_spec_claims.clear();
        let result = jsonwebtoken::decode::<TestClaims>(&token, &keys2.public, &validation);

        assert!(result.is_err(), "verification with wrong public key should fail");
    }

    #[test]
    fn write_read_roundtrip() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let pub_path = dir.path().join("pub.pem");
        let priv_path = dir.path().join("priv.pem");

        let pair = EcKeyPair::generate().expect("key generation should succeed");
        std::fs::write(&pub_path, &pair.public_key_bytes).unwrap();
        std::fs::write(&priv_path, &pair.private_key_bytes).unwrap();

        // Read back and create JwtKeys.
        let pub_bytes = std::fs::read(&pub_path).unwrap();
        let priv_bytes = std::fs::read(&priv_path).unwrap();
        let reloaded = EcKeyPair::new(pub_bytes, priv_bytes);
        let jwt_keys = JwtKeys::try_from(reloaded).expect("reloaded keys should produce valid JwtKeys");

        // Verify signing still works after read-back.
        let claims = TestClaims {
            sub: "roundtrip".to_string(),
            exp: u64::MAX,
        };
        let token =
            jsonwebtoken::encode(&Header::new(Algorithm::ES256), &claims, &jwt_keys.private).expect("signing failed");

        let mut validation = Validation::new(Algorithm::ES256);
        validation.required_spec_claims.clear();
        jsonwebtoken::decode::<TestClaims>(&token, &jwt_keys.public, &validation)
            .expect("verification after read-back failed");
    }
}
