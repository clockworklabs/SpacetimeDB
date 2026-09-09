use super::PublishRequest;
use crate::deployment::MAX_DEPLOYMENT_BYTES;
use crate::environment::{validate_key, validate_value, MAX_ENV_KEY_BYTES, MAX_ENV_VALUE_BYTES, MAX_ENV_VARS};
use serde::de::{MapAccess, Visitor};
use std::{collections::BTreeMap, fmt};

/// Conservative JSON escaping allowance, independent of decoded metadata/map
/// limits. Checked constant arithmetic fails compilation if limits overflow.
pub const MAX_PUBLISH_REQUEST_BYTES: usize = {
    let metadata = MAX_DEPLOYMENT_BYTES.checked_mul(6).expect("metadata JSON bound");
    let entry = MAX_ENV_KEY_BYTES
        .checked_add(MAX_ENV_VALUE_BYTES)
        .expect("environment entry bytes")
        .checked_mul(6)
        .expect("environment JSON escaping")
        .checked_add(8)
        .expect("environment JSON framing");
    metadata
        .checked_add(MAX_ENV_VARS.checked_mul(entry).expect("environment JSON bound"))
        .expect("publication JSON sections")
        .checked_add(4096)
        .expect("publication JSON framing")
};

/// Never retains a value-bearing parser error, source error, input or digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid publication request")]
pub struct PublishRequestError;

impl PublishRequest {
    /// Decode protected HTTP/journal input. Routes must also bound streamed
    /// bytes and concurrent body ownership before buffering reaches this method.
    pub fn decode(bytes: &[u8]) -> Result<Self, PublishRequestError> {
        if bytes.len() > MAX_PUBLISH_REQUEST_BYTES {
            return Err(PublishRequestError);
        }
        let request: Self = serde_json::from_slice(bytes).map_err(|_| PublishRequestError)?;
        request.validate_structure()?;
        Ok(request)
    }

    /// Structural limits also apply to callers constructing a typed request.
    /// Configured resource policy and selected-module declarations are checked
    /// at publication admission, not inferred from environment key selection.
    pub fn validate_structure(&self) -> Result<(), PublishRequestError> {
        self.manifest.encode().map_err(|_| PublishRequestError)?;
        if self.environment.len() > MAX_ENV_VARS {
            return Err(PublishRequestError);
        }
        for (key, value) in &self.environment {
            validate_key(key).map_err(|_| PublishRequestError)?;
            validate_value(value).map_err(|_| PublishRequestError)?;
        }
        Ok(())
    }
}

pub(super) fn deserialize_environment<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, String>, D::Error> {
    struct EnvironmentVisitor;
    impl<'de> Visitor<'de> for EnvironmentVisitor {
        type Value = BTreeMap<String, String>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a bounded publication environment object")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut input: M) -> Result<Self::Value, M::Error> {
            let mut values = BTreeMap::new();
            while let Some(key) = input.next_key::<String>()? {
                if values.len() >= MAX_ENV_VARS || validate_key(&key).is_err() || values.contains_key(&key) {
                    return Err(serde::de::Error::custom("invalid publication environment"));
                }
                let value = input.next_value::<String>()?;
                if validate_value(&value).is_err() {
                    return Err(serde::de::Error::custom("invalid publication environment"));
                }
                values.insert(key, value);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(EnvironmentVisitor)
}

#[cfg(test)]
mod tests;
