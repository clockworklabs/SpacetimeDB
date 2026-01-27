#![allow(clippy::disallowed_macros)]
use spacetimedb_guard::ensure_binaries_built;
use spacetimedb_smoketests::{have_dotnet, workspace_root};
use std::fs;
use std::process::Command;

/// Ensure that the CLI is able to create and compile a C# project.
/// This test does not depend on a running SpacetimeDB instance.
/// Skips if dotnet 8.0+ is not available.
#[test]
fn test_build_csharp_module() {
    if !have_dotnet() {
        eprintln!("Skipping test_build_csharp_module: dotnet 8.0+ not available");
        return;
    }

    let workspace = workspace_root();
    let bindings = workspace.join("crates/bindings-csharp");
    // CLI is pre-built by artifact dependencies during compilation
    let cli_path = ensure_binaries_built();

    // Install wasi-experimental workload
    let _status = Command::new("dotnet")
        .args(["workload", "install", "wasi-experimental", "--skip-manifest-update"])
        .current_dir(workspace.join("modules"))
        .status()
        .expect("Failed to install wasi workload");
    // This may fail if already installed, so we don't assert success

    // Pack the bindings in Release configuration
    let status = Command::new("dotnet")
        .args(["pack", "-c", "Release"])
        .current_dir(&bindings)
        .status()
        .expect("Failed to pack bindings");
    assert!(status.success(), "Failed to pack C# bindings");

    // Create temp directory for the project
    let tmpdir = tempfile::tempdir().expect("Failed to create temp directory");

    // Initialize C# project
    let output = Command::new(&cli_path)
        .args([
            "init",
            "--non-interactive",
            "--lang=csharp",
            "--project-path",
            tmpdir.path().to_str().unwrap(),
            "csharp-project",
        ])
        .output()
        .expect("Failed to run spacetime init");
    assert!(
        output.status.success(),
        "spacetime init failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let server_path = tmpdir.path().join("spacetimedb");

    // Create nuget.config with local package sources
    // Use <clear /> to avoid inheriting sources from machine/user config
    let packed_projects = ["BSATN.Runtime", "Runtime"];
    let mut sources =
        String::from("    <clear />\n    <add key=\"nuget.org\" value=\"https://api.nuget.org/v3/index.json\" />\n");
    let mut mappings = String::new();

    for project in &packed_projects {
        let path = bindings.join(project).join("bin/Release");
        let package_name = format!("SpacetimeDB.{}", project);
        sources.push_str(&format!(
            "    <add key=\"{}\" value=\"{}\" />\n",
            package_name,
            path.display()
        ));
        mappings.push_str(&format!(
            "    <packageSource key=\"{}\">\n      <package pattern=\"{}\" />\n    </packageSource>\n",
            package_name, package_name
        ));
    }
    // Add fallback for other packages
    mappings.push_str("    <packageSource key=\"nuget.org\">\n      <package pattern=\"*\" />\n    </packageSource>\n");

    let nuget_config = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
{}  </packageSources>
  <packageSourceMapping>
{}  </packageSourceMapping>
</configuration>
"#,
        sources, mappings
    );

    eprintln!("Writing nuget.config contents:\n{}", nuget_config);
    fs::write(server_path.join("nuget.config"), &nuget_config).expect("Failed to write nuget.config");

    // Run dotnet publish
    let output = Command::new("dotnet")
        .args(["publish"])
        .current_dir(&server_path)
        .output()
        .expect("Failed to run dotnet publish");

    assert!(
        output.status.success(),
        "dotnet publish failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
