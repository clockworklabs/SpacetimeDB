use predicates::prelude::*;
use spacetimedb_guard::ensure_binaries_built;
use spacetimedb_smoketests::patch_module_cargo_to_local_bindings;
use std::process::Command;

fn cli_cmd() -> Command {
    Command::new(ensure_binaries_built())
}

/// Bun installs a native tsc.exe shim on Windows instead of npm's tsc.cmd.
/// Generate must execute it and reject type errors before bundling the module.
#[cfg(windows)]
#[test]
fn cli_generate_typechecks_bun_windows_module() -> anyhow::Result<()> {
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
    let out_dir = temp_dir.path().join("module_bindings");
    let generate = || {
        cli_cmd()
            .arg("--config-path")
            .arg(&config_path)
            .args(["generate", "--lang", "typescript", "--module-path"])
            .arg(&module_dir)
            .arg("--out-dir")
            .arg(&out_dir)
            .current_dir(temp_dir.path())
            .output()
    };
    let output = generate()?;
    let diagnostics = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "generate skipped typechecking:\n{diagnostics}"
    );
    assert!(
        diagnostics.contains("TS2322"),
        "generate must report the intentional TypeScript error:\n{diagnostics}"
    );
    assert!(
        !module_dir.join("dist/bundle.js").exists(),
        "type errors must stop bundling"
    );

    std::fs::write(&source_path, valid_source)?;
    let output = generate()?;
    assert!(
        output.status.success(),
        "generate failed after fixing the type error:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        out_dir.join("index.ts").is_file(),
        "generate must produce TypeScript bindings"
    );
    Ok(())
}

/// Standalone-only: this builds a local module and generates bindings without contacting a server.
#[test]
fn cli_generate_with_config_but_no_match_uses_cli_args() {
    // Test that when config exists but doesn't match CLI args, we use CLI args
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");

    // Initialize a new project (creates <project-path>/spacetimedb/)
    let output = cli_cmd()
        .args([
            "init",
            "--non-interactive",
            "--lang",
            "rust",
            "--project-path",
            temp_dir.path().to_str().unwrap(),
            "test-project",
        ])
        .current_dir(temp_dir.path())
        .output()
        .expect("failed to execute");
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let project_dir = temp_dir.path().to_path_buf();
    let module_dir = project_dir.join("spacetimedb");
    patch_module_cargo_to_local_bindings(&module_dir).expect("failed to patch module Cargo.toml");

    // Create a config with a different module-path filter
    let config_content = r#"{
  "generate": [
    {
      "language": "typescript",
      "out-dir": "./config-output",
      "module-path": "config-module-path"
    }
  ]
}"#;
    std::fs::write(module_dir.join("spacetime.json"), config_content).expect("failed to write config");

    // Build the module first
    let output = cli_cmd()
        .args(["build", "--module-path", module_dir.to_str().unwrap()])
        .output()
        .expect("failed to execute");
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_dir = module_dir.join("cli-output");
    std::fs::create_dir(&output_dir).expect("failed to create output dir");

    // Generate with different module-path from CLI - should use CLI args, not config
    let output = cli_cmd()
        .args([
            "generate",
            "--lang",
            "rust",
            "--out-dir",
            output_dir.to_str().unwrap(),
            "--module-path",
            module_dir.to_str().unwrap(),
        ])
        .current_dir(&module_dir)
        .output()
        .expect("failed to execute");
    assert!(
        output.status.success(),
        "generate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify files were generated in the CLI-specified output directory
    assert!(
        predicate::path::exists().eval(&output_dir.join("lib.rs"))
            || predicate::path::exists().eval(&output_dir.join("mod.rs")),
        "Generated files should exist in CLI-specified output directory"
    );
}
