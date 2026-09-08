//! Complete publish input. Environment values travel only in the request body.
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::{base64::Base64, serde_as};
use spacetimedb_lib::environment::{validate_key, validate_value, MAX_ENV_VARS};
use std::collections::BTreeMap;

pub const CONTENT_TYPE: &str = "application/vnd.spacetimedb.publish+json";
pub const MAX_MODULE_BYTES: usize = 128 * 1024 * 1024;
/// Includes base64 module expansion and worst-case JSON escaping of configuration.
pub const MAX_REQUEST_BYTES: usize = 192 * 1024 * 1024;

/// Values deliberately have no Debug representation. Omission is an empty map,
/// including for a publish of an unchanged module.
#[serde_as]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishRequest {
    #[serde_as(as = "Base64")]
    pub module: Vec<u8>,
    #[serde(default, deserialize_with = "deserialize_environment")]
    pub environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum PublishRequestError {
    #[error("invalid publish request body")]
    Invalid,
    #[error("publish request exceeds size limit")]
    TooLarge,
}

impl PublishRequest {
    pub fn decode(body: &[u8]) -> Result<Self, PublishRequestError> {
        if body.len() > MAX_REQUEST_BYTES {
            return Err(PublishRequestError::TooLarge);
        }
        // Never expose serde's error text: it can quote a supplied secret.
        let request: Self = serde_json::from_slice(body).map_err(|_| PublishRequestError::Invalid)?;
        request.validate()?;
        Ok(request)
    }

    pub fn encode(&self) -> Result<Vec<u8>, PublishRequestError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|_| PublishRequestError::Invalid)
    }

    fn validate(&self) -> Result<(), PublishRequestError> {
        if self.module.len() > MAX_MODULE_BYTES || self.environment.len() > MAX_ENV_VARS {
            return Err(PublishRequestError::TooLarge);
        }
        for (key, value) in &self.environment {
            validate_key(key).map_err(|_| PublishRequestError::Invalid)?;
            validate_value(value).map_err(|_| PublishRequestError::TooLarge)?;
        }
        Ok(())
    }
}

fn deserialize_environment<'de, D: Deserializer<'de>>(de: D) -> Result<BTreeMap<String, String>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = BTreeMap<String, String>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a complete map of environment strings")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            let mut values = BTreeMap::new();
            while let Some(key) = map.next_key::<String>()? {
                if values.len() >= MAX_ENV_VARS || validate_key(&key).is_err() || values.contains_key(&key) {
                    return Err(A::Error::custom("invalid environment keys"));
                }
                let value = map.next_value::<String>()?;
                if validate_value(&value).is_err() {
                    return Err(A::Error::custom("environment value exceeds size limit"));
                }
                values.insert(key, value);
            }
            Ok(values)
        }
    }
    de.deserialize_map(Visitor)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_and_omission_preserve_complete_string_input() {
        let request = PublishRequest {
            module: vec![0, 1, 255],
            environment: BTreeMap::from([("EMPTY".into(), "".into()), ("TOKEN".into(), "雪\0false".into())]),
        };
        let decoded = PublishRequest::decode(&request.encode().unwrap()).unwrap();
        assert_eq!(decoded.module, request.module);
        assert_eq!(decoded.environment, request.environment);
        assert!(PublishRequest::decode(br#"{"module":""}"#)
            .unwrap()
            .environment
            .is_empty());
    }
    #[test]
    fn malformed_inputs_and_duplicate_keys_are_rejected_without_values() {
        for body in [
            r#"{"module":"","environment":{"KEY":true}}"#,
            r#"{"module":"","environment":{"KEY":null}}"#,
            r#"{"module":"","environment":{"KEY":"first","KEY":"secret-marker"}}"#,
            r#"{"module":"","environment":{"KEY":["secret-marker"]}}"#,
            r#"{"module":"secret-marker"}"#,
            r#"{"module":"","unknown":"secret-marker"}"#,
        ] {
            let error = PublishRequest::decode(body.as_bytes()).err().expect("must reject");
            assert!(!format!("{error:?}: {error}").contains("secret-marker"));
        }
    }
}
