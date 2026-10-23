//! Environment wire types: the `spacetime-environment` and `spacetime-environment-remove`
//! headers, and the metadata returned by `GET /environment`.
use std::collections::BTreeMap;

use http::HeaderValue;
use serde::{Deserialize, Serialize};
use spacetimedb_lib::environment::{validate_key, validate_value, EnvironmentMap, EnvironmentRemove};
use spacetimedb_lib::Hash;

/// Environment values sent with a publish, one `spacetime-environment: KEY=VALUE` header per
/// variable, written like a shell or `.env` assignment.
///
/// A value is bare, single-quoted or double-quoted. Bare and single-quoted values behave as in
/// a shell. A bare value (`API_KEY=abc123`) is taken literally and cannot contain whitespace,
/// quotes, `,` or `\`. A single-quoted value (`GREETING='hello, world'`) is taken literally up to
/// the closing quote, with no escapes. A double-quoted value can contain anything, using JSON
/// string escapes as `.env` files do: `\"`, `\\`, `\n`, `\t` and `\uXXXX` for other control
/// characters and non-ASCII, which cannot appear raw in a header. Commas outside quotes separate
/// entries, so the list stays unambiguous if a proxy merges repeated headers into one
/// comma-separated line.
///
/// Headers are subject to size limits that the environment limits are not: proxies often cap a
/// header line at 8 KiB and all request headers at a few tens of KiB, and hyper rejects requests
/// whose headers total more than about 400 KiB. A large environment can therefore fail to
/// publish even though `PUT /environment` accepts it. If users run into this, publish could also
/// accept a `multipart/form-data` body with the module and the environment as separate parts.
/// That would not break the API: the server would choose the format by `Content-Type`, so raw
/// module bodies with this header would keep working unchanged.
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
            // `to_str` rejects control characters (other than tab) and non-ASCII.
            let value = value.to_str().map_err(|_| err())?;
            for (key, value) in parse_assignments(value).ok_or_else(err)? {
                validate_key(key).map_err(|_| err())?;
                validate_value(&value).map_err(|_| err())?;
                if entries.insert(key.to_owned(), value).is_some() {
                    return Err(err());
                }
            }
        }
        Ok(Self(entries))
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        values.extend(self.0.iter().map(|(key, value)| {
            let mut header = if value.bytes().all(is_bare_value_byte) {
                HeaderValue::try_from(format!("{key}={value}"))
            } else {
                HeaderValue::try_from(format!("{key}={}", quote_value(value)))
            }
            // A valid key, a bare value and a quoted value are all visible ASCII.
            .unwrap();
            header.set_sensitive(true);
            header
        }));
    }
}

/// Whether `byte` may appear in an unquoted value: visible ASCII other than quotes, `,` and `\`.
fn is_bare_value_byte(byte: u8) -> bool {
    byte.is_ascii_graphic() && !matches!(byte, b'"' | b'\'' | b',' | b'\\')
}

/// Quotes `value` as an ASCII-only JSON string.
fn quote_value(value: &str) -> String {
    use std::fmt::Write;
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for c in value.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            ' '..='~' => quoted.push(c),
            _ => {
                for unit in c.encode_utf16(&mut [0; 2]) {
                    let _ = write!(quoted, "\\u{unit:04x}");
                }
            }
        }
    }
    quoted.push('"');
    quoted
}

/// Parses `KEY=VALUE` entries separated by commas, with optional whitespace around entries.
/// Empty entries are ignored, as RFC 9110 requires for list-based headers.
/// Returns `None` if the list is malformed.
fn parse_assignments(mut rest: &str) -> Option<Vec<(&str, String)>> {
    let mut entries = Vec::new();
    loop {
        rest = rest.trim_start_matches([' ', '\t', ',']);
        if rest.is_empty() {
            return Some(entries);
        }
        let (key, after_key) = rest.split_once('=')?;
        let (value, after_value) = if let Some(quoted) = after_key.strip_prefix('\'') {
            let (value, after_quote) = quoted.split_once('\'')?;
            (value.to_owned(), after_quote)
        } else if after_key.starts_with('"') {
            let end = quoted_len(after_key)?;
            let value = serde_json::from_str::<String>(&after_key[..end]).ok()?;
            (value, &after_key[end..])
        } else {
            let end = after_key.find(',').unwrap_or(after_key.len());
            // Trailing whitespace belongs to the list syntax, not the value.
            let value = after_key[..end].trim_end_matches([' ', '\t']);
            if !value.bytes().all(is_bare_value_byte) {
                return None;
            }
            (value.to_owned(), &after_key[end..])
        };
        entries.push((key, value));
        rest = after_value.trim_start_matches([' ', '\t']);
        match rest.strip_prefix(',') {
            Some(after_comma) => rest = after_comma,
            None if rest.is_empty() => return Some(entries),
            None => return None,
        }
    }
}

/// The length of the quoted string at the start of `s`, including both quotes.
fn quoted_len(s: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, byte) in s.bytes().enumerate().skip(1) {
        match byte {
            _ if escaped => escaped = false,
            b'\\' => escaped = true,
            b'"' => return Some(i + 1),
            _ => {}
        }
    }
    None
}

/// Environment keys to remove, as a comma-separated list of key names (`API_KEY, _PRIVATE`),
/// or `*` on its own to remove every stored value.
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
        let err = headers::Error::invalid;
        let mut entries = Vec::new();
        let mut remove_all = false;
        for value in values {
            let value = value.to_str().map_err(|_| err())?;
            for key in value.split(',').map(str::trim).filter(|key| !key.is_empty()) {
                // `*` means "remove everything" only when it stands alone. Rejecting it
                // alongside keys keeps a typo like `FOO, *` from wiping the environment.
                if remove_all || (key == "*" && !entries.is_empty()) {
                    return Err(err());
                }
                if key == "*" {
                    remove_all = true;
                } else {
                    validate_key(key).map_err(|_| err())?;
                    entries.push(key.to_owned());
                }
            }
        }
        if remove_all {
            Ok(Self(EnvironmentRemove::All))
        } else if entries.is_empty() {
            Ok(Self(EnvironmentRemove::No))
        } else {
            Ok(Self(EnvironmentRemove::Keys(entries)))
        }
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        let header = match &self.0 {
            EnvironmentRemove::No => return,
            EnvironmentRemove::All => HeaderValue::from_static("*"),
            // Valid keys are visible ASCII without commas.
            EnvironmentRemove::Keys(keys) => HeaderValue::try_from(keys.join(", ")).unwrap(),
        };
        values.extend([header]);
    }
}

/// Authorized environment metadata. Values never leave the database in this response.
#[derive(Clone, Serialize, Deserialize)]
pub struct EnvironmentMetadata {
    pub module_hash: Hash,
    pub declarations: Vec<spacetimedb_lib::environment::EnvironmentDeclaration>,
    pub stored_keys: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use headers::Header;

    fn encode_environment(entries: &[(&str, &str)]) -> Vec<String> {
        let env = entries.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let mut values = Vec::new();
        SpacetimeEnvironment(env).encode(&mut values);
        values.iter().map(|v| v.to_str().unwrap().to_owned()).collect()
    }

    fn decode_environment(values: &[&str]) -> Result<EnvironmentMap, headers::Error> {
        let values: Vec<HeaderValue> = values.iter().map(|v| HeaderValue::try_from(*v).unwrap()).collect();
        SpacetimeEnvironment::decode(&mut values.iter()).map(|header| header.0)
    }

    #[test]
    fn environment_header_is_one_shell_style_line_per_variable() {
        let entries = [
            ("API_KEY", "abc123"),
            ("URL", "https://x.io/a?b=1&c=%20"),
            ("GREETING", "h\u{e9}llo, \"world\" \\ \u{1f600}"),
            ("PEM", "line1\nline2\0\x7f"),
            ("_PADDED", "  padded  "),
            ("EMPTY", ""),
            ("QUOTE", "it's"),
        ];
        let encoded = encode_environment(&entries);
        assert_eq!(
            encoded,
            [
                "API_KEY=abc123",
                "EMPTY=",
                r#"GREETING="h\u00e9llo, \"world\" \\ \ud83d\ude00""#,
                r#"PEM="line1\nline2\u0000\u007f""#,
                r#"QUOTE="it's""#,
                "URL=https://x.io/a?b=1&c=%20",
                r#"_PADDED="  padded  ""#,
            ]
        );
        let expected: EnvironmentMap = entries.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let lines: Vec<&str> = encoded.iter().map(String::as_str).collect();
        assert_eq!(decode_environment(&lines).unwrap(), expected);
        // A proxy may merge repeated headers into one comma-separated line.
        assert_eq!(decode_environment(&[&lines.join(", ")]).unwrap(), expected);
    }

    #[test]
    fn environment_header_accepts_hand_written_assignments() {
        let decoded = decode_environment(&[
            r#"A=1,B="two words" , C="x,y",D="""#,
            "\tE=5\t",
            r#"F='it, "is" \n literal', G=''"#,
            ", ,H=8,,",
        ])
        .unwrap();
        let expected: EnvironmentMap = [
            ("A", "1"),
            ("B", "two words"),
            ("C", "x,y"),
            ("D", ""),
            ("E", "5"),
            ("F", r#"it, "is" \n literal"#),
            ("G", ""),
            ("H", "8"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(decoded, expected);
    }

    #[test]
    fn environment_header_rejects_malformed_entries() {
        assert_eq!(decode_environment(&[]).unwrap(), EnvironmentMap::new());
        for bad in [
            &["NO_EQUALS"][..],
            &["1BAD=x"],
            &["BAD-KEY=x"],
            &["A =x"],
            &["A=1", "A=2"],
            &["A=1, A=2"],
            &["A=has space"],
            &["API_KEY=abc MODE=production"],
            &["A=has\"quote"],
            &["A=it's"],
            &["A='unterminated"],
            &["A='x'y"],
            &["A=back\\slash"],
            &[r#"A="unterminated"#],
            &[r#"A="x"y"#],
            &[r#"A="bad \q escape""#],
            &["A=\"raw\ttab\""],
        ] {
            assert!(decode_environment(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    fn decode_remove(values: &[&'static str]) -> Result<EnvironmentRemove, headers::Error> {
        let values: Vec<HeaderValue> = values.iter().map(|v| HeaderValue::from_static(v)).collect();
        SpacetimeEnvironmentRemove::decode(&mut values.iter()).map(|header| header.0)
    }

    #[test]
    fn remove_all_must_stand_alone() {
        assert_eq!(decode_remove(&[]).unwrap(), EnvironmentRemove::No);
        assert_eq!(decode_remove(&["*"]).unwrap(), EnvironmentRemove::All);
        let keys = EnvironmentRemove::Keys(vec!["FOO".into(), "_bar".into()]);
        assert_eq!(decode_remove(&["FOO, _bar"]).unwrap(), keys);
        assert_eq!(decode_remove(&["FOO", "_bar"]).unwrap(), keys);
        let mut encoded = Vec::new();
        SpacetimeEnvironmentRemove(keys).encode(&mut encoded);
        assert_eq!(encoded, [HeaderValue::from_static("FOO, _bar")]);
        for bad in [
            &["FOO, *"][..],
            &["*, FOO"],
            &["*, *"],
            &["*", "FOO"],
            &["FOO", "*"],
            &["\"FOO\""],
            &["BAD-KEY"],
        ] {
            assert!(decode_remove(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
