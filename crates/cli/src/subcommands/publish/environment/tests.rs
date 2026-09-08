use super::*;
use spacetimedb_lib::environment::{EnvironmentConstraint as Constraint, EnvironmentDeclaration as Declaration};

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
fn declared_shell_overrides_are_complete_and_redacted() {
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
fn missing_required_never_reuses_old_values_and_optional_disappears() {
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
    let error = resolve(&schema(), Some(&serde_json::json!({"A":"first","B":true})), |_| None)
        .err()
        .unwrap();
    assert!(error.to_string().contains('C'));
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
    let error = resolve(&schema(), Some(&serde_json::json!({"UNDECLARED":"secret"})), |_| {
        panic!("must fail first")
    })
    .err()
    .unwrap();
    assert!(error.to_string().contains("UNDECLARED"));
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

// These fixtures invoke only a locally generated executable in an owned tempdir.
// No CLI config, server credentials or user environment are imported.
#[cfg(unix)]
fn inspector(script: &str) -> (tempfile::TempDir, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("inspector");
    std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    (dir, path)
}

#[cfg(unix)]
#[tokio::test]
async fn local_inspection_passes_exact_bytes_host_and_requires_success() {
    use spacetimedb_lib::db::raw_def::v10::{RawModuleDefV10, RawModuleDefV10Section};
    let raw = RawModuleDef::V10(RawModuleDefV10 {
        sections: vec![RawModuleDefV10Section::Environment(schema().into_declarations())],
    });
    let json = serde_json::to_string(&SerdeWrapper(raw)).unwrap();
    let (dir, extractor) = inspector(&format!(
        "[ \"$1\" = extract-schema ] && [ \"$3\" = --host-type ] && [ \"$4\" = js ] || exit 2\n[ \"$(/bin/cat \"$2\")\" = exact-artifact ] || exit 3\nprintf '%s' '{}'", json.replace('\'', "'\\''")
    ));
    let result = inspect_with(
        extractor,
        b"exact-artifact".to_vec(),
        "Js".into(),
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert_eq!(result.environment(), &schema());
    drop(dir);
    let (_dir, extractor) = inspector(&format!("printf '%s' '{}'; exit 9", json.replace('\'', "'\\''")));
    assert!(
        inspect_with(extractor, b"anything".to_vec(), "Wasm".into(), Duration::from_secs(5))
            .await
            .is_err()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn invalid_and_oversize_inspector_output_never_becomes_diagnostics() {
    for script in [
        "printf 'generated-inspector-secret'",
        "exec /usr/bin/head -c 16777217 /dev/zero",
    ] {
        let (_dir, extractor) = inspector(script);
        let error = inspect_with(extractor, b"input".to_vec(), "Wasm".into(), Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(!format!("{error:#}").contains("generated-inspector-secret"));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn inspection_timeout_and_dropped_waiter_reap_exact_child() {
    // A shell-only busy loop has no descendants and records the exact child PID.
    // kill(0) via the owned process is not used for proof: wait for /bin/kill -0
    // to report ESRCH after our owner has called wait, including cancellation.
    async fn wait_pid(path: &std::path::Path) -> String {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(pid) = tokio::fs::read_to_string(path).await
                    && !pid.trim().is_empty()
                {
                    break pid;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap()
    }
    async fn gone(pid: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let status = tokio::process::Command::new("/bin/kill")
                    .args(["-0", pid.trim()])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .await
                    .unwrap();
                if !status.success() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    for cancel in [false, true] {
        let (dir, extractor) = inspector("placeholder");
        let pid_file = dir.path().join("pid");
        std::fs::write(
            &extractor,
            format!(
                "#!/bin/sh\nprintf '%s' \"$$\" > '{}'\nwhile :; do :; done\n",
                pid_file.display()
            ),
        )
        .unwrap();
        let operation = tokio::spawn(inspect_with(
            extractor,
            b"input".to_vec(),
            "Wasm".into(),
            if cancel {
                Duration::from_secs(30)
            } else {
                Duration::from_millis(300)
            },
        ));
        let pid = wait_pid(&pid_file).await;
        if cancel {
            operation.abort();
            let _ = operation.await;
        } else {
            assert!(operation.await.unwrap().is_err());
        }
        gone(&pid).await;
    }
}

#[tokio::test]
#[ignore = "requires explicit locally built ENV-aware standalone and declared Wasm fixture paths"]
async fn actual_precompiled_declarations_are_inspected_without_server_or_values() {
    let extractor =
        PathBuf::from(std::env::var_os("SPACETIMEDB_ENV_CLI_TEST_EXTRACTOR").expect("explicit inspector path"));
    let module = PathBuf::from(std::env::var_os("SPACETIMEDB_ENV_CLI_TEST_MODULE").expect("explicit module path"));
    assert!(extractor.is_absolute() && module.is_absolute());
    let program = read_program(&module).unwrap();
    let inspected = inspect_with(extractor, program, "Wasm".into(), INSPECT_TIMEOUT)
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
    assert!(resolve(schema, None, |_| None).is_err());
}
