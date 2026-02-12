use assert_cmd::cargo::cargo_bin_cmd;

#[test]
fn cli_generate_with_config_but_no_match_uses_cli_args() {
    // Test that when config exists but doesn't match CLI args, we use CLI args
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");

    // Initialize a new project (creates test-project/spacetimedb/)
    let mut init_cmd = cargo_bin_cmd!("spacetimedb-cli");
    init_cmd
        .args(["init", "--non-interactive", "--lang", "rust", "test-project"])
        .current_dir(temp_dir.path())
        .assert()
        .success();

    let project_dir = temp_dir.path().join("test-project");
    let module_dir = project_dir.join("spacetimedb");

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
    let mut build_cmd = cargo_bin_cmd!("spacetimedb-cli");
    build_cmd
        .args(["build", "--project-path", module_dir.to_str().unwrap()])
        .assert()
        .success();

    let output_dir = module_dir.join("cli-output");
    std::fs::create_dir(&output_dir).expect("failed to create output dir");

    // Generate with different module-path from CLI - should use CLI args, not config
    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args([
        "generate",
        "--lang",
        "rust",
        "--out-dir",
        output_dir.to_str().unwrap(),
        "--project-path",
        module_dir.to_str().unwrap(),
    ])
    .current_dir(&module_dir)
    .assert()
    .success();

    // Verify files were generated in the CLI-specified output directory
    assert!(
        output_dir.join("lib.rs").exists() || output_dir.join("mod.rs").exists(),
        "Generated files should exist in CLI-specified output directory"
    );
}
