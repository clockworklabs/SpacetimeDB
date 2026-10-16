use jsonwebtoken::{errors::Error as JwtError, Algorithm, EncodingKey, Header};
use serde::Serialize;

/// Signs tokens without validating their claims.
pub trait TokenSigner: Sync + Send {
    /// Serialize the claims and sign a JWT with them as the payload.
    fn sign<T: Serialize>(&self, claims: &T) -> Result<String, JwtError>;
}

impl TokenSigner for EncodingKey {
    fn sign<T: Serialize>(&self, claims: &T) -> Result<String, JwtError> {
        jsonwebtoken::encode(&Header::new(Algorithm::ES256), claims, self)
    }
}
