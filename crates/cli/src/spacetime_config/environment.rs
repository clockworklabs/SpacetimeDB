//! Preserve JSON numeric values before the JSON5 deserializer can round them.
//! This also permits existing comments, unquoted names and trailing commas.
use serde_json::Value;

pub(super) fn parse(content: &str) -> anyhow::Result<Value> {
    let bytes = content.as_bytes();
    let mut numbers = Vec::new();
    let mut replacements = Vec::new();
    let mut strings: Vec<String> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        match bytes[i] {
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                    } else if bytes[i] == quote {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i += 2;
                while i < bytes.len() {
                    let ch = content[i..].chars().next().unwrap();
                    if matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                        break;
                    }
                    i += ch.len_utf8();
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && &bytes[i..i + 2] != b"*/" {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            b'{' | b'}' | b'[' | b']' | b':' | b',' => i += 1,
            _ if is_space(content[i..].chars().next().unwrap()) => i += content[i..].chars().next().unwrap().len_utf8(),
            _ => {
                while i < bytes.len()
                    && !is_space(content[i..].chars().next().unwrap())
                    && !matches!(bytes[i], b'{' | b'}' | b'[' | b']' | b':' | b',' | b'/' | b'\'' | b'"')
                {
                    i += content[i..].chars().next().unwrap().len_utf8();
                }
                if i == start {
                    i += content[i..].chars().next().unwrap().len_utf8();
                }
                let token = &content[start..i];
                if (token.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '-' | '+' | '.'))
                    || matches!(token, "Infinity" | "NaN"))
                    && !content[i..].trim_start_matches(is_space).starts_with(':')
                {
                    replacements.push((start, i));
                    numbers.push(token);
                    continue;
                }
            }
        }
        if matches!(bytes[start], b'\'' | b'"') {
            strings.push(json5::from_str(&content[start..i]).map_err(|_| anyhow::anyhow!("Invalid JSON5 string"))?);
        }
    }
    // Check decoded strings as well: Unicode escapes must not manufacture an
    // internal marker and cause a string to be interpreted as a number.
    let mut prefix = "__spacetime_numeric_".to_owned();
    while content.contains(&prefix) || strings.iter().any(|s| s.contains(&prefix)) {
        prefix.push('_');
    }
    let mut text = String::with_capacity(content.len());
    let mut previous = 0;
    for (index, (start, end)) in replacements.into_iter().enumerate() {
        text.push_str(&content[previous..start]);
        text.push_str(&format!("\"{prefix}{index}\""));
        previous = end;
    }
    text.push_str(&content[previous..]);
    // Parser diagnostics may quote the source line, which can contain secrets.
    let mut value: Value = json5::from_str(&text).map_err(|_| anyhow::anyhow!("Invalid JSON5 configuration"))?;
    restore(&mut value, &prefix, &numbers, None)?;
    Ok(value)
}

/// Deserialize through JSON text so Serde's flattened-field buffer does not
/// receive visit_u128 from Value's deserializer. That buffer cannot represent
/// u128, while the arbitrary-precision JSON parser preserves its decimal token.
/// Diagnostics deliberately discard the original error, which may quote values.
pub(super) fn decode_config(value: Value) -> anyhow::Result<super::SpacetimeConfig> {
    let encoded = serde_json::to_vec(&value).map_err(|_| anyhow::anyhow!("Invalid configuration structure"))?;
    serde_json::from_slice(&encoded).map_err(|_| anyhow::anyhow!("Invalid configuration structure"))
}

fn is_space(ch: char) -> bool {
    ch.is_whitespace() || ch == '\u{feff}'
}

fn restore(value: &mut Value, prefix: &str, numbers: &[&str], env_key: Option<&str>) -> anyhow::Result<()> {
    match value {
        Value::String(s) => {
            if let Some(index) = s.strip_prefix(prefix).and_then(|s| s.parse::<usize>().ok()) {
                let token = numbers[index];
                *value = if let Some(env_key) = env_key {
                    Value::Number(token.parse().map_err(|_| {
                        anyhow::anyhow!(
                            "Environment key {:?}: numeric config input must use JSON number syntax",
                            env_key
                        )
                    })?)
                } else {
                    // Preserve existing JSON5 conveniences outside env. JSON numbers retain
                    // arbitrary precision throughout layering and config serialization.
                    token
                        .parse()
                        .map(Value::Number)
                        .or_else(|_| json5::from_str(token))
                        .map_err(|_| anyhow::anyhow!("Invalid numeric configuration input"))?
                };
            }
        }
        Value::Object(object) => {
            for (key, value) in object {
                if key == "env" && env_key.is_none() {
                    if let Value::Object(env) = value {
                        for (name, value) in env {
                            restore(value, prefix, numbers, Some(name))?;
                        }
                    } else {
                        restore(value, prefix, numbers, Some("env"))?;
                    }
                } else {
                    restore(value, prefix, numbers, env_key)?;
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                restore(value, prefix, numbers, env_key)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Merge only object-valued env maps. Invalid higher precedence input remains
/// invalid instead of silently falling back to the lower layer.
pub(super) fn overlay(base: &mut Value, higher: &Value) {
    if let (Some(base), Some(higher)) = (base.as_object_mut(), higher.as_object()) {
        base.extend(higher.clone());
    } else {
        *base = higher.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacetime_config::{find_and_load_with_env_from, SpacetimeConfig};
    #[test]
    fn numeric_input_is_lossless_with_json5_comments_and_strings() {
        let value = parse(
            r#"{ // comment 111
            env: { HUGE: 9007199254740993123456789, DECIMAL: 0.1234567890123456789012345,
                   EXP: 1.000000000000000000001e+300, STRING: '12 // 99', BOOL: false, },
            'server': 'http://127.0.0.1:9', /* 222 */ 'num-replicas': 3,
        }"#,
        )
        .unwrap();
        assert_eq!(value["env"]["HUGE"].to_string(), "9007199254740993123456789");
        assert_eq!(value["env"]["DECIMAL"].to_string(), "0.1234567890123456789012345");
        assert_eq!(value["env"]["EXP"].to_string(), "1.000000000000000000001e+300");
        assert_eq!(value["env"]["STRING"], "12 // 99");
        assert_eq!(value["num-replicas"], 3);
        let unicode_lines = parse("{ // comment\u{2028} env: { NUMBER: 9007199254740993123456789\u{feff}} }").unwrap();
        assert_eq!(unicode_lines["env"]["NUMBER"].to_string(), "9007199254740993123456789");
    }
    #[test]
    fn numeric_extensions_and_parse_errors_do_not_quote_secret_input() {
        for input in ["NaN", "Infinity", "-Infinity", "0xFF", "+2", ".5"] {
            assert!(parse(&format!("{{env: {{KEY: {input}}}}}")).is_err());
        }
        let error = parse("{env: {KEY: 'generated-secret-sentinel', broken }").unwrap_err();
        assert!(!format!("{error:#}").contains("generated-secret-sentinel"));
        let value = parse("{env: {A: '__spacetime_numeric_0', B: 2}}").unwrap();
        assert_eq!(value["env"]["A"], "__spacetime_numeric_0");
        assert_eq!(value["env"]["B"], 2);
        let escaped = parse(r#"{env: {A: '\u005f_spacetime_numeric_999', B: 2}}"#).unwrap();
        assert_eq!(escaped["env"]["A"], "__spacetime_numeric_999");
    }
    #[test]
    fn four_layers_and_parent_child_merge_individual_keys() {
        let dir = tempfile::tempdir().unwrap();
        for (file, json) in [
            (
                "spacetime.json",
                r#"{database:'parent', env:{A:'base',B:1},children:[{database:'child',env:{B:2,C:'child'}}]}"#,
            ),
            (
                "spacetime.local.json",
                r#"{env:{A:'local'},children:[{env:{D:'local-child'}}]}"#,
            ),
            (
                "spacetime.prod.json",
                r#"{env:{A:'prod',E:true},children:[{env:{B:3}}]}"#,
            ),
            (
                "spacetime.prod.local.json",
                r#"{env:{A:'prod-local'},children:[{env:{}}]}"#,
            ),
        ] {
            std::fs::write(dir.path().join(file), json).unwrap();
        }
        let config = find_and_load_with_env_from(Some("prod"), dir.path().to_owned())
            .unwrap()
            .unwrap();
        let targets = config.config.collect_all_targets_with_inheritance();
        assert_eq!(
            targets[0].fields["env"],
            serde_json::json!({"A":"prod-local","B":1,"E":true})
        );
        assert_eq!(
            targets[1].fields["env"],
            serde_json::json!({"A":"prod-local","B":3,"C":"child","D":"local-child","E":true})
        );
        // Direct loads use the same lossless parser.
        let base = SpacetimeConfig::load(&dir.path().join("spacetime.json")).unwrap();
        assert_eq!(base.additional_fields["env"]["B"], 1);
    }
    #[test]
    fn loaded_flattened_configuration_preserves_full_precision_and_redacts_structure_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("spacetime.json");
        for number in [
            "9007199254740993123456789",
            "0.1234567890123456789012345",
            "1.000000000000000000001e+300",
        ] {
            std::fs::write(
                &path,
                format!(r#"{{"database":"owned","env":{{"NUMBER":{number}}},"children":[{{"database":"child"}}]}}"#),
            )
            .unwrap();
            let direct = SpacetimeConfig::load(&path).unwrap();
            let layered = find_and_load_with_env_from(None, dir.path().to_owned())
                .unwrap()
                .unwrap()
                .config;
            for config in [direct, layered] {
                for target in config.collect_all_targets_with_inheritance() {
                    assert_eq!(target.fields["env"]["NUMBER"].to_string(), number);
                }
            }
        }
        std::fs::write(
            &path,
            r#"{"children":"private-structure-sentinel","env":{"NUMBER":9007199254740993123456789}}"#,
        )
        .unwrap();
        for error in [
            SpacetimeConfig::load(&path).unwrap_err(),
            find_and_load_with_env_from(None, dir.path().to_owned()).err().unwrap(),
        ] {
            let error = format!("{error:#}");
            assert!(!error.contains("private-structure-sentinel"));
            assert!(!error.contains("9007199254740993123456789"));
        }
    }

    #[test]
    fn invalid_higher_layer_is_not_an_empty_map_or_fallback() {
        let mut base = serde_json::json!({"A":"base"});
        overlay(&mut base, &Value::Null);
        assert!(base.is_null());
        let config: SpacetimeConfig =
            serde_json::from_value(serde_json::json!({"env":{"A":1},"children":[{"env":null}]})).unwrap();
        assert!(config.collect_all_targets_with_inheritance()[1].fields["env"].is_null());
    }
}
