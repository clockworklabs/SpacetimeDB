//! CLI dev command tests

use predicates::prelude::*;
use spacetimedb_guard::ensure_binaries_built;
use std::process::Command;

fn cli_cmd() -> Command {
    Command::new(ensure_binaries_built())
}

#[test]
fn cli_dev_reuses_server_issued_login_for_log_streaming() {
    use spacetimedb_smoketests::{require_server_issued_login, Smoketest};
    use std::fs;
    use std::io::Write;
    use std::process::{Child, Stdio};
    use std::time::{Duration, Instant};

    require_server_issued_login!();
    let test = Smoketest::builder()
        .autopublish(false)
        .module_code(
            r#"
#[spacetimedb::reducer(init)]
pub fn init(_ctx: &spacetimedb::ReducerContext) {
    log::info!("dev-login-log-stream-ready");
}
"#,
        )
        .build();
    // Start dev without the login created by the smoketest harness.
    fs::write(&test.config_path, "").unwrap();
    let stdout = tempfile::NamedTempFile::new().unwrap();
    let stderr = tempfile::NamedTempFile::new().unwrap();

    struct DevProcess(Child);
    impl Drop for DevProcess {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    let mut dev = DevProcess(
        cli_cmd()
            .current_dir(test.project_dir.path())
            .arg("--config-path")
            .arg(&test.config_path)
            .args([
                "dev",
                "dev-server-issued-login",
                "--server",
                &test.server_url,
                "--module-path",
                ".",
                "--server-only",
                "--skip-generate",
                "--no-config",
            ])
            .stdin(Stdio::piped())
            .stdout(stdout.reopen().unwrap())
            .stderr(stderr.reopen().unwrap())
            .spawn()
            .unwrap(),
    );
    // Decline spacetimedb.com login and explicitly accept a server-issued identity.
    dev.0.stdin.take().unwrap().write_all(b"n\ny\n").unwrap();

    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let out = fs::read_to_string(stdout.path()).unwrap();
        let err = fs::read_to_string(stderr.path()).unwrap();
        if out.contains("dev-login-log-stream-ready") {
            break;
        }
        let status = dev.0.try_wait().unwrap();
        assert!(
            status.is_none() && Instant::now() < deadline,
            "dev did not reach log streaming (status: {status:?}):\nstdout: {out}\nstderr: {err}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Standalone-only: this checks local CLI help output, so a remote cluster adds no coverage.
#[test]
fn cli_dev_help_shows_template_option() {
    let output = cli_cmd().args(["dev", "--help"]).output().expect("failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        predicate::str::contains("--template").eval(&stdout),
        "stdout should contain --template"
    );
    assert!(predicate::str::contains("-t").eval(&stdout), "stdout should contain -t");
}

/// Standalone-only: this checks local argument parsing before any server request is made.
#[test]
fn cli_dev_accepts_template_flag() {
    // Running with an invalid server should fail, but not because of the template flag
    let output = cli_cmd()
        .args(["dev", "--template", "react", "--server", "nonexistent-server-12345"])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The error should be about the server, not about an unrecognized --template flag
    assert!(
        !stderr.contains("unrecognized") || !stderr.contains("template"),
        "stderr should not complain about unrecognized template flag"
    );
}

/// Standalone-only: this checks local argument parsing before any server request is made.
#[test]
fn cli_dev_accepts_short_template_flag() {
    let output = cli_cmd()
        .args(["dev", "-t", "typescript", "--server", "nonexistent-server-12345"])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The error should be about the server, not about an unrecognized -t flag
    assert!(
        !stderr.contains("unrecognized") || !stderr.contains("-t"),
        "stderr should not complain about unrecognized -t flag"
    );
}

/// Standalone-only: this initializes and inspects a project entirely on the local filesystem.
#[test]
fn cli_init_with_template_creates_project() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");

    let output = cli_cmd()
        .current_dir(temp_dir.path())
        .args([
            "init",
            "--template",
            "basic-rs",
            "--local",
            "--non-interactive",
            "test-project",
        ])
        .output()
        .expect("failed to execute");

    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify expected files were created
    let project_dir = temp_dir.path().join("test-project");
    assert!(
        project_dir.join("spacetimedb").exists(),
        "spacetimedb directory should exist"
    );
    assert!(project_dir.join("src").exists(), "src directory should exist");
}

/// Standalone-only: this validates local config parsing before the CLI contacts a server.
#[test]
fn config_with_invalid_field_shows_error() {
    // Test that using invalid field names shows a helpful error message
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");

    // Create a config with an invalid field name in dev
    let config_content = r#"{
  "dev": {
    "run_command": "npm run dev"
  },
  "publish": {
    "database": "test-db"
  }
}"#;
    std::fs::write(temp_dir.path().join("spacetime.json"), config_content).expect("failed to write config");

    // Create minimal spacetimedb module
    std::fs::create_dir(temp_dir.path().join("spacetimedb")).expect("failed to create spacetimedb dir");
    std::fs::create_dir(temp_dir.path().join("spacetimedb/src")).expect("failed to create src dir");
    std::fs::write(
        temp_dir.path().join("spacetimedb/Cargo.toml"),
        r#"[package]
name = "test"
version = "0.1.0"

[dependencies]
spacetimedb = "1.0"

[lib]
crate-type = ["cdylib"]
"#,
    )
    .expect("failed to write Cargo.toml");
    std::fs::write(temp_dir.path().join("spacetimedb/src/lib.rs"), "").expect("failed to write lib.rs");

    let output = cli_cmd()
        .current_dir(temp_dir.path())
        .args(["dev", "test-db"])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success(), "dev should fail with invalid config field");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Failed to load spacetime.json"),
        "stderr should mention Failed to load spacetime.json"
    );
    assert!(
        stderr.contains("unknown field `run_command`"),
        "stderr should mention unknown field run_command"
    );
}
