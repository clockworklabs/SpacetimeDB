#![cfg(windows)]

use spacetimedb_guard::ensure_binaries_built;
use std::process::Command;

fn cli_cmd() -> Command {
    Command::new(ensure_binaries_built())
}

/// Bun installs a native tsc.exe shim on Windows instead of npm's tsc.cmd.
/// Build must execute it and reject type errors before bundling the module.
#[test]
fn cli_build_typechecks_bun_windows_module() -> anyhow::Result<()> {
    use spacetimedb_smoketests::{build_typescript_sdk, workspace_root};

    let temp_dir = tempfile::tempdir()?;
    let config_path = temp_dir.path().join("config.toml");
    let output = cli_cmd()
        .arg("--config-path")
        .arg(&config_path)
        .args(["init", "--non-interactive", "--lang", "typescript", "--project-path"])
        .arg(temp_dir.path())
        .arg("bun-typecheck")
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
            "name": "bun-typecheck-module",
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
    assert!(!bin_dir.join("tsc.cmd").exists(), "tsc.cmd would mask the regression");

    let source_path = module_dir.join("src/index.ts");
    let valid_source = std::fs::read_to_string(&source_path)?;
    std::fs::write(
        &source_path,
        format!("{valid_source}\nconst bunTypecheckRegression: number = \"wrong\";\n"),
    )?;
    let build = || {
        cli_cmd()
            .arg("--config-path")
            .arg(&config_path)
            .args(["build", "--module-path"])
            .arg(&module_dir)
            .current_dir(temp_dir.path())
            .output()
    };
    let output = build()?;
    let diagnostics = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.status.success(), "build skipped typechecking:\n{diagnostics}");
    assert!(
        diagnostics.contains("TS2322"),
        "build must report the intentional TypeScript error:\n{diagnostics}"
    );
    assert!(
        !module_dir.join("dist/bundle.js").exists(),
        "type errors must stop bundling"
    );

    std::fs::write(&source_path, valid_source)?;
    let output = build()?;
    assert!(
        output.status.success(),
        "build failed after fixing the type error:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        module_dir.join("dist/bundle.js").is_file(),
        "build must produce the module bundle"
    );
    Ok(())
}
