//! Atomic publish input. Environment values travel only in the request body.
use serde::{Deserialize, Deserializer, Serialize};
use spacetimedb_lib::environment::{validate_key, validate_value, MAX_ENV_VARS};
use spacetimedb_lib::Hash;
use std::collections::BTreeMap;

pub const CONTENT_TYPE: &str = "application/vnd.spacetimedb.publish+json";
pub const MAX_MODULE_BYTES: usize = 128 * 1024 * 1024;
/// Includes base64 module expansion and worst-case JSON escaping of configuration.
pub const MAX_REQUEST_BYTES: usize = 192 * 1024 * 1024;

/// Values deliberately have no Debug representation. Omission is an empty map,
/// including for a publish of an unchanged module.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentPublish {
    #[serde(default, deserialize_with = "deserialize_environment")]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub environment_remove: Vec<String>,
    #[serde(default)]
    pub environment_replace: bool,
    #[serde(default)]
    pub expected_module_version: Option<String>,
}

pub struct SpacetimeEnvironment(pub BTreeMap<String, String>);

impl headers::Header for SpacetimeEnvironment {
    fn name() -> &'static http::HeaderName {
        static NAME: http::HeaderName = http::HeaderName::from_static("spacetime-environment");
        &NAME
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i http::HeaderValue>,
    {
        let mut entries = BTreeMap::new();
        for value in values {
            let dict = sfv::Parser::new(value)
                .with_version(sfv::Version::Rfc9651)
                .parse::<sfv::Dictionary>()
                .map_err(|_| headers::Error::invalid())?;
            for (k, v) in dict {
                validate_key(k.as_str()).map_err(|_| headers::Error::invalid())?;
                let v = match v {
                    sfv::ListEntry::Item(sfv::Item { bare_item, params }) if params.is_empty() => bare_item,
                    _ => return Err(headers::Error::invalid()),
                };
                let v = match v {
                    sfv::BareItem::String(s) => s.into(),
                    sfv::BareItem::DisplayString(s) => s,
                    _ => return Err(headers::Error::invalid()),
                };
                validate_value(&v).map_err(|_| headers::Error::invalid())?;
                entries.insert(k.into(), v);
            }
        }
        Ok(Self(entries))
    }

    fn encode<E: Extend<http::HeaderValue>>(&self, values: &mut E) {
        let mut ser = sfv::DictSerializer::new();
        for (k, v) in &self.0 {
            let _ = ser.bare_item(k.as_str().try_into().unwrap(), sfv::RefBareItem::DisplayString(v));
        }
        if let Some(header) = ser.finish() {
            values.extend([header.try_into().unwrap()]);
        }
    }
}

// pub struct SpacetimeEnvironmentRemove(Vec<String>);

// impl headers::Header for SpacetimeEnvironmentRemove {
//     fn name() -> &'static http::HeaderName {
//         static NAME: http::HeaderName = http::HeaderName::from_static("spacetime-environment-remove");
//         &NAME
//     }

//     fn decode<'i, I>(values: &mut I) -> Result<Self, headers::Error>
//     where
//         Self: Sized,
//         I: Iterator<Item = &'i http::HeaderValue>,
//     {
//         let mut entries = Vec::new();
//         for value in values {
//             let list = sfv::Parser::new(value)
//                 .with_version(sfv::Version::Rfc9651)
//                 .parse::<sfv::List>()
//                 .map_err(|_| headers::Error::invalid())?;
//             entries.reserve(list.len());
//             for v in list {
//                 let tok = match v {
//                     sfv::ListEntry::Item(sfv::Item {
//                         bare_item: sfv::BareItem::Token(tok),
//                         params,
//                     }) if params.is_empty() => tok,
//                     _ => return Err(headers::Error::invalid()),
//                 };
//                 entries.push(tok.into());
//             }
//         }
//         Ok(Self(entries))
//     }

//     fn encode<E: Extend<http::HeaderValue>>(&self, values: &mut E) {
//         let mut ser = sfv::ListSerializer::new();
//         for v in &self.0 {
//             let _ = ser.bare_item(sfv::TokenRef::from_str(v).unwrap());
//         }
//         if let Some(header) = ser.finish() {
//             values.extend([header.try_into().unwrap()]);
//         }
//     }
// }

/// Authorized environment metadata. Values never leave the database in this response.
#[derive(Clone, Serialize, Deserialize)]
pub struct EnvironmentMetadata {
    pub module_version: String,
    pub declarations: Vec<spacetimedb_lib::environment::EnvironmentDeclaration>,
    pub stored_keys: Vec<String>,
}

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum PublishRequestError {
    #[error("invalid publish request body")]
    Invalid,
    #[error("publish request exceeds size limit")]
    TooLarge,
}

impl EnvironmentPublish {
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
        if self.environment.len() > MAX_ENV_VARS {
            return Err(PublishRequestError::TooLarge);
        }
        spacetimedb_lib::environment::EnvironmentUpdate {
            values: self.environment.clone(),
            remove: self.environment_remove.clone(),
            replace: self.environment_replace,
        }
        .validate()
        .map_err(|_| PublishRequestError::Invalid)?;
        if self
            .expected_module_version
            .as_ref()
            .is_some_and(|hash| hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(PublishRequestError::Invalid);
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
            f.write_str("a map of supplied environment strings")
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
        let request = EnvironmentPublish {
            environment: BTreeMap::from([("EMPTY".into(), "".into()), ("TOKEN".into(), "雪\0false".into())]),
            ..Default::default()
        };
        let decoded = EnvironmentPublish::decode(&request.encode().unwrap()).unwrap();
        assert_eq!(decoded.environment, request.environment);
        assert!(EnvironmentPublish::decode(br#"{}"#).unwrap().environment.is_empty());
    }
    #[test]
    fn environment_only_mutation_roundtrips_and_rejects_conflicting_operations() {
        let request = EnvironmentPublish {
            environment: BTreeMap::from([("FUTURE".into(), "secret-marker".into())]),
            environment_remove: vec!["OPTIONAL".into()],
            expected_module_version: Some("ab".repeat(32)),
            ..Default::default()
        };
        let bytes = request.encode().unwrap();
        assert!(serde_json::from_slice::<serde_json::Value>(&bytes)
            .unwrap()
            .get("module")
            .is_none());
        let decoded = EnvironmentPublish::decode(&bytes).unwrap();
        assert_eq!(decoded.environment_remove, request.environment_remove);
        assert_eq!(decoded.expected_module_version, request.expected_module_version);
        for body in [
            r#"{"environment_replace":true,"environment_remove":["KEY"]}"#,
            r#"{"environment":{"KEY":"secret-marker"},"environment_remove":["KEY"]}"#,
            r#"{"environment_remove":["KEY","KEY"]}"#,
            r#"{"expected_module_version":"secret-marker"}"#,
        ] {
            let error = EnvironmentPublish::decode(body.as_bytes()).err().expect("must reject");
            assert!(!error.to_string().contains("secret-marker"));
        }
    }

    #[test]
    fn malformed_inputs_and_duplicate_keys_are_rejected_without_values() {
        for body in [
            r#"{"environment":{"KEY":true}}"#,
            r#"{"environment":{"KEY":null}}"#,
            r#"{"environment":{"KEY":"first","KEY":"secret-marker"}}"#,
            r#"{"environment":{"KEY":["secret-marker"]}}"#,
            r#"{"unknown":"secret-marker"}"#,
        ] {
            let error = EnvironmentPublish::decode(body.as_bytes()).err().expect("must reject");
            assert!(!format!("{error:?}: {error}").contains("secret-marker"));
        }
    }
}
