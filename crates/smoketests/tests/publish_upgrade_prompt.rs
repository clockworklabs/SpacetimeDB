use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{bail, Context, Result};
use regex::Regex;
use spacetimedb_guard::{ensure_binaries_built, SpacetimeDbGuard};
use spacetimedb_smoketests::{random_string, workspace_root};

const MODULE_CODE: &str = r#"
use spacetimedb::{reducer, ReducerContext};

#[reducer]
pub fn noop(_ctx: &ReducerContext) {}
"#;

fn create_module_project(project_dir: &Path, crate_name: &str, bindings_path: &Path) -> Result<()> {
    let bindings_path = bindings_path.display().to_string().replace('\\', "/");
    let cargo_toml = format!(
        r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
spacetimedb = {{ path = "{bindings_path}", features = ["unstable"] }}
log = "0.4"
"#
    );

    fs::create_dir_all(project_dir.join("src")).context("create module src dir")?;
    fs::write(project_dir.join("Cargo.toml"), cargo_toml).context("write module Cargo.toml")?;
    fs::write(project_dir.join("src/lib.rs"), MODULE_CODE).context("write module lib.rs")?;
    Ok(())
}

fn build_module(project_dir: &Path, crate_name: &str, target_dir: &Path) -> Result<PathBuf> {
    let output = Command::new("cargo")
        .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
        .env("CARGO_TARGET_DIR", target_dir)
        .current_dir(project_dir)
        .output()
        .context("run cargo build for module")?;

    if !output.status.success() {
        bail!(
            "module build failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let wasm = target_dir
        .join("wasm32-unknown-unknown")
        .join("release")
        .join(format!("{crate_name}.wasm"));
    if !wasm.exists() {
        bail!("built wasm not found at {}", wasm.display());
    }
    Ok(wasm)
}

fn old_fixture_wasm() -> PathBuf {
    workspace_root()
        .join("crates")
        .join("smoketests")
        .join("fixtures")
        .join("upgrade_old_module_v1.wasm")
}

fn run_publish(
    cli: &Path,
    config_path: &Path,
    server_url: &str,
    db_name: &str,
    wasm_path: &Path,
    stdin_data: &str,
) -> Result<Output> {
    let mut child = Command::new(cli)
        .arg("--config-path")
        .arg(config_path)
        .args(["publish", "--anonymous", "--yes", "--server", server_url, "--bin-path"])
        .arg(wasm_path)
        .arg(db_name)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn spacetime publish")?;

    {
        let stdin = child.stdin.as_mut().context("child missing stdin")?;
        use std::io::Write;
        stdin
            .write_all(stdin_data.as_bytes())
            .context("write publish stdin input")?;
    }

    child.wait_with_output().context("wait on spacetime publish")
}

fn parse_identity(stdout: &str) -> Option<String> {
    let re = Regex::new(r"identity: ([0-9a-fA-F]+)").ok()?;
    let caps = re.captures(stdout)?;
    Some(caps.get(1)?.as_str().to_string())
}

#[test]
fn publish_upgrade_prompt_wasm_end_to_end() {
    let cli = ensure_binaries_built();
    let guard = SpacetimeDbGuard::spawn_in_temp_data_dir();
    let tmp = tempfile::tempdir().unwrap();
    let tmp_path = tmp.path();
    let config_path = tmp_path.join("config.toml");
    fs::write(&config_path, "").unwrap();

    let old_wasm = old_fixture_wasm();
    assert!(old_wasm.exists(), "expected old fixture wasm at {}", old_wasm.display());

    let new_project = tmp_path.join("new-module");
    let new_crate = "upgrade_new_module";
    create_module_project(
        &new_project,
        new_crate,
        &workspace_root().join("crates").join("bindings"),
    )
    .unwrap();
    let new_wasm = build_module(&new_project, new_crate, &tmp_path.join("target-new")).unwrap();

    let db_name = format!("upgrade-smoke-{}", random_string());

    let first = run_publish(&cli, &config_path, &guard.host_url, &db_name, &old_wasm, "").unwrap();
    assert!(
        first.status.success(),
        "initial old-module publish failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(
        parse_identity(&first_stdout).is_some(),
        "could not parse identity from first publish stdout:\n{}",
        first_stdout
    );

    let deny = run_publish(&cli, &config_path, &guard.host_url, &db_name, &new_wasm, "no\n").unwrap();
    assert!(
        !deny.status.success(),
        "expected publish denial to fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&deny.stdout),
        String::from_utf8_lossy(&deny.stderr)
    );
    let deny_stdout = String::from_utf8_lossy(&deny.stdout);
    assert!(deny_stdout.contains("major version upgrade from 1.0 to 2.0"));
    assert!(deny_stdout.contains("Please type 'upgrade' to accept this change:"));

    let accept = run_publish(&cli, &config_path, &guard.host_url, &db_name, &new_wasm, "upgrade\n").unwrap();
    assert!(
        accept.status.success(),
        "expected publish accept to succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&accept.stdout),
        String::from_utf8_lossy(&accept.stderr)
    );
    let accept_stdout = String::from_utf8_lossy(&accept.stdout);
    assert!(accept_stdout.contains("major version upgrade from 1.0 to 2.0"));
    assert!(accept_stdout.contains("Please type 'upgrade' to accept this change:"));
    assert!(accept_stdout.contains("Updated database") || accept_stdout.contains("Created new database"));
}
