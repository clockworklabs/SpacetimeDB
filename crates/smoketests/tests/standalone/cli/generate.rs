use predicates::prelude::*;
use spacetimedb_guard::ensure_binaries_built;
use spacetimedb_smoketests::{patch_module_cargo_to_local_bindings, workspace_root};
use std::process::Command;

fn cli_cmd() -> Command {
    Command::new(ensure_binaries_built())
}

fn copy_module_fixture(name: &str, dest: &std::path::Path) {
    let source = workspace_root().join("crates/smoketests/modules").join(name);
    std::fs::create_dir_all(dest.join("src")).expect("failed to create copied module src dir");
    std::fs::copy(source.join("Cargo.toml"), dest.join("Cargo.toml")).expect("failed to copy Cargo.toml");
    std::fs::copy(source.join("src/lib.rs"), dest.join("src/lib.rs")).expect("failed to copy lib.rs");
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

/// Standalone-only: receiver/module bindings are generated from a local Rust module without contacting a server.
#[test]
fn cli_generate_rust_module_bindings() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");

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

    let module_dir = temp_dir.path().join("spacetimedb");
    patch_module_cargo_to_local_bindings(&module_dir).expect("failed to patch module Cargo.toml");

    let output = cli_cmd()
        .args(["build", "--module-path", module_dir.to_str().unwrap()])
        .output()
        .expect("failed to execute");
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_dir = module_dir.join("receiver-bindings");
    let output = cli_cmd()
        .args([
            "generate",
            "--lang",
            "rust",
            "--bindings",
            "module",
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

    let root = std::fs::read_to_string(output_dir.join("lib.rs")).expect("generated lib.rs should exist");
    assert!(root.contains("pub struct Identity(pub spacetimedb::Identity);"));
    assert!(root.contains("impl spacetimedb::rt::RemoteReducer for add"));
    assert!(root.contains("const NAME: &'static str = \"add\";"));
    assert!(!root.contains("DbConnection"));
    assert!(!output_dir.join("add_reducer.rs").exists());
}

/// Standalone-only: a sender module can use generated receiver bindings from a spacetime.json dependency.
#[test]
fn cli_generate_receiver_bindings_from_dependency_compile_in_sender() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let receiver_dir = temp_dir.path().join("receiver");
    let sender_dir = temp_dir.path().join("sender");

    copy_module_fixture("idc-receiver", &receiver_dir);
    copy_module_fixture("idc-sender", &sender_dir);
    patch_module_cargo_to_local_bindings(&receiver_dir).expect("failed to patch receiver Cargo.toml");
    patch_module_cargo_to_local_bindings(&sender_dir).expect("failed to patch sender Cargo.toml");

    std::fs::write(
        temp_dir.path().join("spacetime.json"),
        r#"{
  "server": "local",
  "children": [
    {
      "database": "game-world",
      "module-path": "./receiver"
    },
    {
      "database": "lobby",
      "module-path": "./sender",
      "dependencies": [
        { "name": "game_world", "database": "game-world" }
      ]
    }
  ]
}
"#,
    )
    .expect("failed to write spacetime.json");

    let output = cli_cmd()
        .args(["generate", "lobby"])
        .current_dir(temp_dir.path())
        .output()
        .expect("failed to execute");
    assert!(
        output.status.success(),
        "generate failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let remote_bindings_mod = std::fs::read_to_string(sender_dir.join("src/remote_bindings/mod.rs"))
        .expect("generated remote_bindings mod.rs should exist");
    assert!(remote_bindings_mod.contains("pub mod game_world;"));

    let generated = sender_dir.join("src/remote_bindings/game_world/mod.rs");
    let root = std::fs::read_to_string(&generated).expect("generated receiver binding should exist");
    assert!(root.contains("pub struct Identity(pub spacetimedb::Identity);"));
    assert!(root.contains("impl spacetimedb::rt::RemoteReducer for receive_ping"));
    assert!(root.contains("receive_ping"));

    let output = cli_cmd()
        .args(["build", "--module-path", sender_dir.to_str().unwrap()])
        .output()
        .expect("failed to execute");
    assert!(
        output.status.success(),
        "sender build failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
