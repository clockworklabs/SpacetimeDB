use predicates::prelude::*;
use spacetimedb_guard::ensure_binaries_built;
use spacetimedb_smoketests::patch_module_cargo_to_local_bindings;
use std::process::Command;

fn cli_cmd() -> Command {
    Command::new(ensure_binaries_built())
}

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
