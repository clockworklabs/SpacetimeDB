use super::*;
use spacetimedb_lib::environment::{EnvironmentConstraint as Constraint, EnvironmentDeclaration as Declaration};
use std::{path::PathBuf, time::Duration};

fn schema() -> EnvironmentSchema {
    EnvironmentSchema::new(vec![
        Declaration {
            name: "A".into(),
            constraint: Constraint::AnyString,
            optional: false,
        },
        Declaration {
            name: "B".into(),
            constraint: Constraint::OneOf(vec!["true".into(), "false".into()]),
            optional: false,
        },
        Declaration {
            name: "C".into(),
            constraint: Constraint::AnyString,
            optional: false,
        },
        Declaration {
            name: "OPTIONAL".into(),
            constraint: Constraint::AnyString,
            optional: true,
        },
    ])
    .unwrap()
}

#[test]
fn declared_shell_overrides_are_redacted() {
    let mut checked = Vec::new();
    let resolved = resolve(
        &schema(),
        Some(&serde_json::json!({"A":"config-sentinel","B":false})),
        |name| {
            checked.push(name.to_owned());
            match name {
                "C" => Some("shell-sentinel".into()),
                "A" => Some("".into()),
                _ => None,
            }
        },
    )
    .unwrap();
    assert_eq!(
        resolved.values,
        BTreeMap::from([
            ("A".into(), "".into()),
            ("B".into(), "false".into()),
            ("C".into(), "shell-sentinel".into())
        ])
    );
    assert_eq!(checked, vec!["A", "B", "C", "OPTIONAL"]);
    assert_eq!(
        resolved.display(),
        "Environment A (shell)\nEnvironment B (config)\nEnvironment C (shell)\n"
    );
    assert!(!resolved.display().contains("sentinel"));
    // No declaration means no ambient lookup, including PATH or credentials.
    let empty = resolve(&EnvironmentSchema::default(), None, |_| panic!("ambient access")).unwrap();
    assert!(empty.values.is_empty());
}

#[test]
fn omitted_inputs_are_left_for_host_to_resolve() {
    let config = serde_json::json!({"A":"first","B":true,"C":"first","OPTIONAL":"old"});
    let first = resolve(&schema(), Some(&config), |_| None).unwrap();
    assert!(first.values.contains_key("OPTIONAL"));
    let second = resolve(
        &schema(),
        Some(&serde_json::json!({"A":"next","B":false,"C":"next"})),
        |_| None,
    )
    .unwrap();
    assert!(!second.values.contains_key("OPTIONAL"));
    let partial = resolve(&schema(), Some(&serde_json::json!({"A":"first","B":true})), |_| None).unwrap();
    assert!(!partial.values.contains_key("C"));
    assert!(resolve(&schema(), None, |_| None).unwrap().values.is_empty());
}

#[test]
fn invalid_inputs_fail_without_values_or_lower_priority_fallback() {
    for input in [Value::Null, serde_json::json!([]), serde_json::json!({})] {
        let config = serde_json::json!({"A":input,"B":true,"C":"private-sentinel"});
        let error = resolve(&schema(), Some(&config), |_| None).err().unwrap();
        assert!(!format!("{error:#}").contains("private-sentinel"));
    }
    let config = serde_json::json!({"A":"private-sentinel","B":false,"C":"private-sentinel"});
    let error = resolve(&schema(), Some(&config), |name| {
        (name == "B").then(|| "invalid-shell-secret".into())
    })
    .err()
    .unwrap();
    let error = format!("{error:#}");
    assert!(error.contains('B'));
    assert!(!error.contains("invalid-shell-secret") && !error.contains("private-sentinel"));
    let mut looked_up = Vec::new();
    let resolved = resolve(&schema(), Some(&serde_json::json!({"UNDECLARED":"secret"})), |key| {
        looked_up.push(key.to_owned());
        None
    })
    .unwrap();
    assert_eq!(resolved.values["UNDECLARED"], "secret");
    assert!(!looked_up.iter().any(|key| key == "UNDECLARED"));
    assert!(!resolved.display().contains("secret"));
}

#[test]
fn env_layer_selector_is_not_a_value_source() {
    let command = super::super::cli();
    let args = command
        .clone()
        .try_get_matches_from(["publish", "db", "--env", "prod"])
        .unwrap();
    let schema = super::super::build_publish_schema(&command).unwrap();
    let config = crate::spacetime_config::CommandConfig::new(
        &schema,
        std::collections::HashMap::from([("env".into(), serde_json::json!({"A":"from-config"}))]),
        &args,
    )
    .unwrap();
    assert_eq!(config.get_config_value("env").unwrap()["A"], "from-config");
    assert!(!config.is_from_cli("env"));
}

#[test]
fn number_boolean_and_empty_string_conversion_has_no_float_rounding() {
    let schema = EnvironmentSchema::new(
        ["NUMBER", "BOOL", "EMPTY"]
            .map(|name| Declaration {
                name: name.into(),
                constraint: Constraint::AnyString,
                optional: false,
            })
            .to_vec(),
    )
    .unwrap();
    let config = serde_json::from_str(r#"{"NUMBER":9007199254740993123456789,"BOOL":false,"EMPTY":""}"#).unwrap();
    let resolved = resolve(&schema, Some(&config), |_| None).unwrap();
    assert_eq!(resolved.values["NUMBER"], "9007199254740993123456789");
    assert_eq!(resolved.values["BOOL"], "false");
    assert_eq!(resolved.values["EMPTY"], "");
}

#[cfg(unix)]
#[test]
fn non_utf8_declared_shell_value_is_rejected_without_bytes() {
    use std::os::unix::ffi::OsStringExt;
    let error = resolve(&schema(), None, |name| {
        (name == "A").then(|| OsString::from_vec(vec![0xff, 0xfe]))
    })
    .err()
    .unwrap();
    assert!(error.to_string().contains("UTF-8"));
}

#[tokio::test]
#[ignore = "requires explicit locally built ENV-aware standalone and declared Wasm fixture paths"]
async fn actual_precompiled_declarations_are_inspected_without_server_or_values() {
    let extractor =
        PathBuf::from(std::env::var_os("SPACETIMEDB_ENV_CLI_TEST_EXTRACTOR").expect("explicit inspector path"));
    let module = PathBuf::from(std::env::var_os("SPACETIMEDB_ENV_CLI_TEST_MODULE").expect("explicit module path"));
    assert!(extractor.is_absolute() && module.is_absolute());
    let program = read_program(&module).unwrap();
    let inspected = crate::schema_extract::inspect_with(extractor, program, "Wasm".into(), Duration::from_secs(60))
        .await
        .unwrap();
    let schema = inspected.environment();
    assert!(inspected.environment_declared());
    assert!(!schema.get("REQUIRED").unwrap().optional);
    assert_eq!(
        schema.get("MODE").unwrap().constraint,
        Constraint::OneOf(vec!["other".into(), "ready".into()])
    );
    let config = serde_json::json!({"REQUIRED":"generated-local-inspection-sentinel","MODE":"ready"});
    let resolved = resolve(schema, Some(&config), |_| None).unwrap();
    assert_eq!(resolved.values.len(), 2);
    assert!(!resolved.display().contains("generated-local-inspection-sentinel"));
    assert!(resolve(schema, None, |_| None).unwrap().values.is_empty());
}

#[test]
fn undeclared_inputs_still_obey_storage_limits_and_redact_invalid_keys() {
    for input in [
        serde_json::json!({"private-marker\ninvalid":null}),
        serde_json::json!({"UNDECLARED":"x".repeat(8193)}),
    ] {
        let error = resolve(&EnvironmentSchema::default(), Some(&input), |_| {
            panic!("no declared shell lookup")
        })
        .err()
        .unwrap();
        assert!(!format!("{error:#}").contains("private-marker"));
    }
    let input = Value::Object(
        (0..257)
            .map(|i| (format!("K{i}"), Value::String(String::new())))
            .collect(),
    );
    assert!(resolve(&EnvironmentSchema::default(), Some(&input), |_| None).is_err());
}
