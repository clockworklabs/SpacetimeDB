#![cfg(windows)]

use assert_cmd::prelude::*;
use spacetimedb_guard::ensure_binaries_built;
use std::process::Command;

fn cli_cmd() -> Command {
    Command::new(ensure_binaries_built())
}

/// Bun installs a native tsc.exe shim on Windows instead of npm's tsc.cmd.
/// Verify a TypeScript module installed with Bun builds successfully.
#[test]
fn cli_build_bun_windows_module() -> anyhow::Result<()> {
    use spacetimedb_smoketests::{build_typescript_sdk, workspace_root};

    let temp_dir = tempfile::tempdir()?;
    let config_path = temp_dir.path().join("config.toml");
    let project_dir = temp_dir.path().join("bun-build");
    cli_cmd()
        .arg("--config-path")
        .arg(&config_path)
        .args(["init", "--non-interactive", "--lang", "typescript", "--project-path"])
        .arg(&project_dir)
        .arg("bun-build")
        .current_dir(temp_dir.path())
        .assert()
        .success();

    build_typescript_sdk()?;
    let module_dir = project_dir.join("spacetimedb");
    let sdk_path = workspace_root().join("crates/bindings-typescript");
    std::fs::write(
        module_dir.join("package.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "name": "bun-build-module",
            "private": true,
            "dependencies": { "spacetimedb": format!("file:{}", sdk_path.to_string_lossy().replace('\\', "/")) },
            "devDependencies": { "typescript": "5.9.3" }
        }))?,
    )?;
    Command::new("bun")
        .arg("install")
        .current_dir(&module_dir)
        .assert()
        .success();
    cli_cmd()
        .arg("--config-path")
        .arg(&config_path)
        .args(["build", "--module-path"])
        .arg(&module_dir)
        .current_dir(temp_dir.path())
        .assert()
        .success();
    Ok(())
}
