#![cfg(windows)]

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
    let output = cli_cmd()
        .arg("--config-path")
        .arg(&config_path)
        .args(["init", "--non-interactive", "--lang", "typescript", "--project-path"])
        .arg(temp_dir.path())
        .arg("bun-build")
        .current_dir(temp_dir.path())
        .output()?;
    assert!(
        output.status.success(),
        "init failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    build_typescript_sdk()?;
    let module_dir = temp_dir.path().join("spacetimedb");
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
    let output = Command::new("bun").arg("install").current_dir(&module_dir).output()?;
    assert!(
        output.status.success(),
        "bun install failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let bin_dir = module_dir.join("node_modules/.bin");
    assert!(bin_dir.join("tsc.exe").is_file(), "Bun must install tsc.exe");
    assert!(
        !bin_dir.join("tsc.cmd").exists(),
        "Bun should install tsc.exe rather than tsc.cmd"
    );

    let output = cli_cmd()
        .arg("--config-path")
        .arg(&config_path)
        .args(["build", "--module-path"])
        .arg(&module_dir)
        .current_dir(temp_dir.path())
        .output()?;
    assert!(
        output.status.success(),
        "build failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
