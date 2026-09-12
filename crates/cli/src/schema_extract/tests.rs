use super::*;
use spacetimedb_lib::environment::{
    EnvironmentConstraint as Constraint, EnvironmentDeclaration as Declaration, EnvironmentSchema,
};

#[tokio::test]
async fn inspector_environment_excludes_secrets_and_preserves_windows_dll_search() {
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "schema_extract::tests::inspector_environment_child",
            "--ignored",
        ])
        .env("STDB_INSPECTOR_TEST_SECRET", "must-not-reach-inspector");
    configure_inspector_environment(&mut command);
    let explicit: Vec<_> = command.as_std().get_envs().collect();
    #[cfg(windows)]
    {
        let path = std::env::var_os("PATH");
        let expected: Vec<_> = path
            .as_deref()
            .map(|value| (std::ffi::OsStr::new("PATH"), Some(value)))
            .into_iter()
            .collect();
        assert_eq!(explicit, expected);
    }
    #[cfg(not(windows))]
    assert!(explicit.is_empty());
    let output = command.output().await.unwrap();
    assert!(
        output.status.success(),
        "isolated inspector process failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
#[ignore = "invoked by the isolated inspector environment regression"]
fn inspector_environment_child() {
    // CoreFoundation adds this variable during process initialization on macOS,
    // even when the parent supplies an empty environment.
    #[cfg(target_os = "macos")]
    assert!(std::env::vars_os().all(|(key, _)| key == "__CF_USER_TEXT_ENCODING"));
    #[cfg(windows)]
    assert!(std::env::vars_os().all(|(key, _)| key.as_encoded_bytes().eq_ignore_ascii_case(b"PATH")));
    #[cfg(all(not(windows), not(target_os = "macos")))]
    assert!(std::env::vars_os().next().is_none(), "unexpected inherited variable");
}

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
#[tokio::test(flavor = "current_thread")]
async fn synchronous_generate_adapter_uses_same_exact_byte_protocol_inside_a_runtime() {
    use spacetimedb_lib::db::raw_def::v10::{RawModuleDefV10, RawModuleDefV10Section};
    let raw = RawModuleDef::V10(RawModuleDefV10 {
        sections: vec![RawModuleDefV10Section::Environment(schema().into_declarations())],
    });
    let json = serde_json::to_string(&SerdeWrapper(raw)).unwrap();
    let (_dir, extractor) = inspector(&format!(
        "[ \"$1\" = extract-schema ] && [ \"$3\" = --host-type ] && [ \"$4\" = wasm ] || exit 2\n[ \"$(/bin/cat \"$2\")\" = generate-exact ] || exit 3\nprintf '%s' '{}'", json.replace('\'', "'\\''")
    ));
    let module = inspect_blocking(extractor, b"generate-exact".to_vec(), "Wasm".into()).unwrap();
    assert_eq!(module.environment(), &schema());
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
async fn timeout_and_caller_drop_produce_positive_wait_receipts() {
    for cancel in [false, true] {
        let (_dir, extractor) = inspector("while :; do :; done");
        let (started, started_receipt) = tokio::sync::oneshot::channel();
        let (reaped, reaped_receipt) = tokio::sync::oneshot::channel();
        let operation = tokio::spawn(inspect_observed(
            extractor,
            b"input".to_vec(),
            "Wasm".into(),
            if cancel {
                Duration::from_secs(30)
            } else {
                Duration::from_millis(300)
            },
            Observation {
                started: Some(started),
                reaped: Some(reaped),
            },
        ));
        assert!(
            tokio::time::timeout(Duration::from_secs(5), started_receipt)
                .await
                .unwrap()
                .unwrap()
                > 0
        );
        if cancel {
            operation.abort();
            let _ = operation.await;
        } else {
            assert!(operation.await.unwrap().is_err());
        }
        let status = tokio::time::timeout(Duration::from_secs(5), reaped_receipt)
            .await
            .unwrap()
            .unwrap();
        assert!(!status.success());
    }
}
