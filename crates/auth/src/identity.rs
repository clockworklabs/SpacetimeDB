pub use jsonwebtoken::errors::Error as JwtError;
pub use jsonwebtoken::errors::ErrorKind as JwtErrorKind;
pub use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::Deserializer;
use serde::{Deserialize, Serialize};
use spacetimedb_lib::Identity;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct ConnectionAuthCtx {
    pub claims: SpacetimeIdentityClaims,
    pub jwt_payload: String,
}

impl TryFrom<SpacetimeIdentityClaims> for ConnectionAuthCtx {
    type Error = anyhow::Error;
    fn try_from(claims: SpacetimeIdentityClaims) -> Result<Self, Self::Error> {
        let payload = serde_json::to_string(&claims).map_err(|e| anyhow::anyhow!("Failed to serialize claims: {e}"))?;
        Ok(ConnectionAuthCtx {
            claims,
            jwt_payload: payload,
        })
    }
}

// These are the claims that can be attached to a request/connection.
#[serde_with::serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SpacetimeIdentityClaims {
    #[serde(rename = "hex_identity")]
    pub identity: Identity,
    #[serde(rename = "sub")]
    pub subject: String,
    #[serde(rename = "iss")]
    pub issuer: String,
    #[serde(rename = "aud")]
    pub audience: Vec<String>,

    /// The unix timestamp the token was issued at
    #[serde_as(as = "serde_with::TimestampSeconds")]
    pub iat: SystemTime,
    #[serde_as(as = "Option<serde_with::TimestampSeconds>")]
    pub exp: Option<SystemTime>,
}

fn deserialize_audience<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    // By using `untagged`, it will try the different options.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Audience {
        Single(String),
        Multiple(Vec<String>),
    }

    // Deserialize into the enum
    let audience = Audience::deserialize(deserializer)?;

    // Convert the enum into a Vec<String>
    Ok(match audience {
        Audience::Single(s) => vec![s],
        Audience::Multiple(v) => v,
    })
}

// IncomingClaims are from the token we receive from the client.
// The signature should be verified already, but further validation is needed to have a SpacetimeIdentityClaims2.
#[serde_with::serde_as]
#[derive(Debug, Serialize, Deserialize)]
pub struct IncomingClaims {
    #[serde(rename = "hex_identity")]
    pub identity: Option<Identity>,
    #[serde(rename = "sub")]
    pub subject: String,
    #[serde(rename = "iss")]
    pub issuer: String,
    #[serde(rename = "aud", default, deserialize_with = "deserialize_audience")]
    pub audience: Vec<String>,

    /// The unix timestamp the token was issued at
    #[serde_as(as = "serde_with::TimestampSeconds")]
    pub iat: SystemTime,
    #[serde_as(as = "Option<serde_with::TimestampSeconds>")]
    pub exp: Option<SystemTime>,
}

impl TryInto<SpacetimeIdentityClaims> for IncomingClaims {
    type Error = anyhow::Error;

    fn try_into(self) -> anyhow::Result<SpacetimeIdentityClaims> {
        // The issuer and subject must be less than 128 bytes.
        if self.issuer.len() > 128 {
            return Err(anyhow::anyhow!("Issuer too long: {:?}", self.issuer));
        }
        if self.subject.len() > 128 {
            return Err(anyhow::anyhow!("Subject too long: {:?}", self.subject));
        }
        // The issuer and subject must be non-empty.
        if self.issuer.is_empty() {
            return Err(anyhow::anyhow!("Issuer empty"));
        }
        if self.subject.is_empty() {
            return Err(anyhow::anyhow!("Subject empty"));
        }

        let computed_identity = Identity::from_claims(&self.issuer, &self.subject);
        // If an identity is provided, it must match the computed identity.
        if let Some(token_identity) = self.identity {
            if token_identity != computed_identity {
                return Err(anyhow::anyhow!(
                    "Identity mismatch: token identity {token_identity:?} does not match computed identity {computed_identity:?}",
                ));
            }
        }

        Ok(SpacetimeIdentityClaims {
            identity: computed_identity,
            subject: self.subject,
            issuer: self.issuer,
            audience: self.audience,
            iat: self.iat,
            exp: self.exp,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::UNIX_EPOCH;

    #[test]
    fn test_deserialize_audience_single_string() {
        let json_data = json!({
            "sub": "123",
            "iss": "example.com",
            "aud": "audience1",
            "iat": 1693425600,
            "exp": 1693512000
        });

        let claims: IncomingClaims = serde_json::from_value(json_data).unwrap();

        assert_eq!(claims.audience, vec!["audience1"]);
        assert_eq!(claims.subject, "123");
        assert_eq!(claims.issuer, "example.com");
        assert_eq!(claims.iat, UNIX_EPOCH + std::time::Duration::from_secs(1693425600));
        assert_eq!(
            claims.exp,
            Some(UNIX_EPOCH + std::time::Duration::from_secs(1693512000))
        );
    }

    #[test]
    fn test_deserialize_audience_multiple_strings() {
        let json_data = json!({
            "sub": "123",
            "iss": "example.com",
            "aud": ["audience1", "audience2"],
            "iat": 1693425600,
            "exp": 1693512000
        });

        let claims: IncomingClaims = serde_json::from_value(json_data).unwrap();

        assert_eq!(claims.audience, vec!["audience1", "audience2"]);
        assert_eq!(claims.subject, "123");
        assert_eq!(claims.issuer, "example.com");
        assert_eq!(claims.iat, UNIX_EPOCH + std::time::Duration::from_secs(1693425600));
        assert_eq!(
            claims.exp,
            Some(UNIX_EPOCH + std::time::Duration::from_secs(1693512000))
        );
    }

    #[test]
    fn test_deserialize_audience_missing_field() {
        let json_data = json!({
            "sub": "123",
            "iss": "example.com",
            "iat": 1693425600,
            "exp": 1693512000
        });

        let claims: IncomingClaims = serde_json::from_value(json_data).unwrap();

        assert!(claims.audience.is_empty()); // Since `default` is used, it should be an empty vector
        assert_eq!(claims.subject, "123");
        assert_eq!(claims.issuer, "example.com");
        assert_eq!(claims.iat, UNIX_EPOCH + std::time::Duration::from_secs(1693425600));
        assert_eq!(
            claims.exp,
            Some(UNIX_EPOCH + std::time::Duration::from_secs(1693512000))
        );
    }
}
