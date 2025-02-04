use jsonwebtoken::{DecodingKey, EncodingKey};
use rcgen::KeyPair;
use spacetimedb_paths::cli::{PrivKeyPath, PubKeyPath};

use crate::config::CertificateAuthority;

pub mod identity;
pub mod token_validation;

/// JWT verification and signing keys.
#[derive(Clone)]
pub struct JwtKeys {
    pub public: DecodingKey,
    pub public_pem: Box<[u8]>,
    pub private: EncodingKey,
    pub kid: Option<String>,
}

impl JwtKeys {
    /// Create a new [`JwtKeys`] from paths to the public and private key files
    /// respectively.
    ///
    /// The key files must be PEM encoded ECDSA P256 keys.
    pub fn new(public_pem: impl Into<Box<[u8]>>, private_pem: &[u8]) -> anyhow::Result<Self> {
        let public_pem = public_pem.into();
        let public = DecodingKey::from_ec_pem(&public_pem)?;
        let private = EncodingKey::from_ec_pem(private_pem)?;

        Ok(Self {
            public,
            private,
            public_pem,
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
        JwtKeys::new(pair.public_key_bytes, &pair.private_key_bytes)
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

        // Get the private key in PKCS#8 PEM format & write it.
        let public_key_bytes = key_pair.public_key_pem().into_bytes();

        // Get the public key in PEM format & write it.
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
