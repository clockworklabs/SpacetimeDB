//! Atomic publish input. Environment values travel only in the request body.
use std::collections::BTreeMap;

use http::HeaderValue;
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
        I: Iterator<Item = &'i HeaderValue>,
    {
        let err = headers::Error::invalid;
        let mut entries = BTreeMap::new();
        for value in values {
            let list = sfv::Parser::new(value)
                .with_version(sfv::Version::Rfc9651)
                .parse::<sfv::List>()
                .map_err(|_| err())?;
            for entry in list {
                let items = match entry {
                    sfv::ListEntry::InnerList(sfv::InnerList { items, params }) if params.is_empty() => items,
                    _ => return Err(err()),
                };
                let [k, v] = <[sfv::Item; 2]>::try_from(items)
                    .ok()
                    .filter(|x| x.iter().all(|item| item.params.is_empty()))
                    .ok_or_else(err)?
                    .map(|x| x.bare_item);
                let k: String = match k {
                    sfv::BareItem::Token(tok) => tok.into(),
                    sfv::BareItem::String(s) => s.into(),
                    _ => return Err(err()),
                };
                let v = match v {
                    sfv::BareItem::String(s) => s.into(),
                    sfv::BareItem::DisplayString(s) => s,
                    _ => return Err(err()),
                };
                validate_key(&k).map_err(|_| err())?;
                validate_value(&v).map_err(|_| err())?;
                entries.insert(k, v);
            }
        }
        Ok(Self(entries))
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        let mut ser = sfv::ListSerializer::new();
        for (k, v) in &self.0 {
            let mut tuple = ser.inner_list();
            // a valid environment key is always a valid sfv::String
            let _ = tuple.bare_item(as_tok_or_string(k).unwrap());
            let _ = tuple.bare_item(sfv::RefBareItem::DisplayString(v));
            let _ = tuple.finish();
        }
        if let Some(header) = ser.finish() {
            let mut header: HeaderValue = header.try_into().unwrap();
            header.set_sensitive(true);
            values.extend([header]);
        }
    }
}

fn as_tok_or_string(s: &str) -> Option<sfv::RefBareItem<'_>> {
    let item = match sfv::TokenRef::from_str(s) {
        Ok(tok) => tok.into(),
        Err(_) => sfv::StringRef::from_str(s).ok()?.into(),
    };
    Some(item)
}

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
        I: Iterator<Item = &'i HeaderValue>,
    {
        let mut entries = Vec::new();
        for value in values {
            let list = sfv::Parser::new(value)
                .with_version(sfv::Version::Rfc9651)
                .parse::<sfv::List>()
                .map_err(|_| headers::Error::invalid())?;
            entries.reserve(list.len());
            for v in list {
                let sfv::ListEntry::Item(sfv::Item { bare_item, params }) = v else {
                    return Err(headers::Error::invalid());
                };
                if !params.is_empty() {
                    return Err(headers::Error::invalid());
                }
                let key = match bare_item {
                    sfv::BareItem::Token(tok) if tok.as_str() == "*" => return Ok(Self(EnvironmentRemove::All)),
                    sfv::BareItem::Token(tok) => tok.into(),
                    sfv::BareItem::String(s) => s.into(),
                    _ => return Err(headers::Error::invalid()),
                };
                entries.push(key)
            }
        }
        if entries.is_empty() {
            Ok(Self(EnvironmentRemove::No))
        } else {
            Ok(Self(EnvironmentRemove::Keys(entries)))
        }
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        match &self.0 {
            EnvironmentRemove::No => {}
            EnvironmentRemove::All => {
                values.extend([const { HeaderValue::from_static("*") }]);
            }
            EnvironmentRemove::Keys(remove) => {
                let mut ser = sfv::ListSerializer::new();
                for key in remove {
                    // a valid environment key is always a valid sfv::String
                    let item = as_tok_or_string(key).unwrap();
                    let _ = ser.bare_item(item);
                }
                if let Some(header) = ser.finish() {
                    values.extend([header.try_into().unwrap()]);
                }
            }
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
