//! Atomic publish input. Environment values travel only in the request body.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use spacetimedb_lib::environment::{validate_key, validate_value, EnvironmentMap, EnvironmentRemove};
use spacetimedb_lib::Hash;

#[derive(Default)]
pub struct SpacetimeEnvironment(pub EnvironmentMap);

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
            let mut header: http::HeaderValue = header.try_into().unwrap();
            header.set_sensitive(true);
            values.extend([header]);
        }
    }
}

// "*" / list(token)
#[derive(Default)]
pub struct SpacetimeEnvironmentRemove(pub EnvironmentRemove);

impl headers::Header for SpacetimeEnvironmentRemove {
    fn name() -> &'static http::HeaderName {
        static NAME: http::HeaderName = http::HeaderName::from_static("spacetime-environment-remove");
        &NAME
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i http::HeaderValue>,
    {
        let mut entries = Vec::new();
        for value in values {
            let list = sfv::Parser::new(value)
                .with_version(sfv::Version::Rfc9651)
                .parse::<sfv::List>()
                .map_err(|_| headers::Error::invalid())?;
            entries.reserve(list.len());
            for v in list {
                let tok = match v {
                    sfv::ListEntry::Item(sfv::Item {
                        bare_item: sfv::BareItem::Token(tok),
                        params,
                    }) if params.is_empty() => tok,
                    _ => return Err(headers::Error::invalid()),
                };
                if tok.as_str() == "*" {
                    return Ok(Self(EnvironmentRemove::All));
                }
                entries.push(tok.into());
            }
        }
        Ok(Self(EnvironmentRemove::Keys(entries)))
    }

    fn encode<E: Extend<http::HeaderValue>>(&self, values: &mut E) {
        let mut ser = sfv::ListSerializer::new();
        match &self.0 {
            EnvironmentRemove::No => {}
            EnvironmentRemove::All => {
                let _ = ser.bare_item(const { sfv::token_ref("*") });
            }
            EnvironmentRemove::Keys(remove) => {
                for v in remove {
                    let _ = ser.bare_item(sfv::TokenRef::from_str(v).unwrap());
                }
            }
        }
        if let Some(header) = ser.finish() {
            values.extend([header.try_into().unwrap()]);
        }
    }
}

/// Authorized environment metadata. Values never leave the database in this response.
#[derive(Clone, Serialize, Deserialize)]
pub struct EnvironmentMetadata {
    pub module_hash: Hash,
    pub declarations: Vec<spacetimedb_lib::environment::EnvironmentDeclaration>,
    pub stored_keys: Vec<String>,
}
