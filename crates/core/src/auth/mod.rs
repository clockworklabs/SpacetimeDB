use jsonwebtoken::{DecodingKey, EncodingKey};
use openssl::ec::{EcGroup, EcKey};
use openssl::nid::Nid;
use openssl::pkey::PKey;
use spacetimedb_paths::cli::{PrivKeyPath, PubKeyPath};

use crate::config::CertificateAuthority;

pub mod identity;
pub mod token_validation;

/// JWT verification and signing keys.
pub struct JwtKeys {
    pub public: DecodingKey,
    pub public_pem: Box<[u8]>,
    pub private: EncodingKey,
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
        })
    }
}

pub fn get_or_create_keys(certs: &CertificateAuthority) -> anyhow::Result<JwtKeys> {
    let public_key_path = &certs.jwt_pub_key_path;
    let private_key_path = &certs.jwt_priv_key_path;

    let public_key_bytes = public_key_path.read().ok();
    let private_key_bytes = private_key_path.read().ok();

    // If both keys are unspecified, create them
    let (public_key_bytes, private_key_bytes) = match (public_key_bytes, private_key_bytes) {
        (Some(pub_), Some(priv_)) => (pub_, priv_),
        (None, None) => create_keys(public_key_path, private_key_path)?,
        (None, Some(_)) => anyhow::bail!("Unable to read public key for JWT token verification"),
        (Some(_), None) => anyhow::bail!("Unable to read private key for JWT token signing"),
    };

    JwtKeys::new(public_key_bytes, &private_key_bytes)
}

pub fn create_keys(public_key_path: &PubKeyPath, private_key_path: &PrivKeyPath) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    // Create a new EC group from a named curve.
    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;

    // Create a new EC key with the specified group.
    let eckey = EcKey::generate(&group)?;

    // Create a new PKey from the EC key.
    let pkey = PKey::from_ec_key(eckey.clone())?;

    // Get the private key in PKCS#8 PEM format & write it.
    let private_key = pkey.private_key_to_pem_pkcs8()?;
    private_key_path.write(&private_key)?;

    // Get the public key in PEM format & write it.
    let public_key = eckey.public_key_to_pem()?;
    public_key_path.write(&public_key)?;

    Ok((public_key, private_key))
}
