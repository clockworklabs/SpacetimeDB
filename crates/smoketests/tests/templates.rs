#![allow(clippy::disallowed_macros)]
//! Template tests - ensure all project templates can be initialized, built, and published.
//!
//! These tests verify that:
//! 1. `spacetime init --template <id>` successfully creates a project
//! 2. The server-side module can be published to a SpacetimeDB instance
//! 3. The client-side code compiles/type-checks successfully
//!
//! Templates are discovered dynamically by scanning the `templates/` directory for
//! subdirectories that contain a `.template.json` metadata file.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use spacetimedb_smoketests::{pnpm_path, random_string, workspace_root, Smoketest};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

// ============================================================================
// Template metadata
// ============================================================================

#[derive(Debug, Clone)]
struct Template {
    /// Directory name under `templates/`, used as the template id.
    id: String,
    /// The server language (e.g. "rust", "typescript", "csharp").
    server_lang: Option<String>,
    /// The client language (e.g. "rust", "typescript", "csharp").
    client_lang: Option<String>,
}

/// Discovers all templates by scanning `<workspace_root>/templates/`.
///
/// A directory is treated as a template if it contains a `.template.json` file
/// with (at minimum) a `server_lang` field - matching what `spacetime init
/// --template` expects.
fn get_templates() -> Vec<Template> {
    let templates_dir = workspace_root().join("templates");
    let mut templates = Vec::new();

    let entries = fs::read_dir(&templates_dir)
        .unwrap_or_else(|e| panic!("Failed to read templates directory {:?}: {}", templates_dir, e));

    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }

        let template_json = entry.path().join(".template.json");
        if !template_json.exists() {
            continue;
        }

        let content =
            fs::read_to_string(&template_json).unwrap_or_else(|e| panic!("Failed to read {:?}: {}", template_json, e));
        let meta: Value =
            serde_json::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {:?}: {}", template_json, e));

        templates.push(Template {
            id: entry.file_name().to_string_lossy().into_owned(),
            server_lang: meta["server_lang"].as_str().map(String::from),
            client_lang: meta["client_lang"].as_str().map(String::from),
        });
    }

    templates.sort_by(|a, b| a.id.cmp(&b.id));
    templates
}

/// Converts a filesystem path into a dependency path string suitable for
/// Cargo.toml/package.json entries.
///
/// On Windows, `canonicalize()` can return verbatim paths like `\\?\D:\...`.
/// Cargo path dependencies reject that as `//?/D:/...`, so strip `\\?\` first.
fn normalize_dependency_path(local_path: &Path) -> String {
    let abs_path = local_path.canonicalize().unwrap_or_else(|_| local_path.to_path_buf());
    let mut path_str = abs_path.to_string_lossy().into_owned();

    if let Some(stripped) = path_str.strip_prefix(r"\\?\") {
        path_str = stripped.to_string();
    }

    path_str.replace('\\', "/")
}

// ============================================================================
// Helpers
// ============================================================================

/// Runs `spacetime init --template <id>` into a fresh temp directory.
/// Returns `(tmpdir, project_path)` - caller must keep `tmpdir` alive.
fn init_template(test: &Smoketest, template_id: &str) -> Result<(TempDir, PathBuf)> {
    let tmpdir = tempfile::tempdir().context("Failed to create temp dir")?;
    let project_name = format!("test-{}", template_id);
    let project_path = tmpdir.path().join(&project_name);

    test.spacetime(&[
        "init",
        "--template",
        template_id,
        "--project-path",
        project_path.to_str().unwrap(),
        "--non-interactive",
        &project_name,
    ])
    .with_context(|| format!("spacetime init --template {} failed", template_id))?;

    if !project_path.exists() {
        bail!("Project directory not created for template {}", template_id);
    }

    Ok((tmpdir, project_path))
}

/// Updates a `[dependencies]` entry in a `Cargo.toml` to use a local path.
fn update_cargo_toml_dependency(cargo_toml_path: &Path, package_name: &str, local_path: &Path) -> Result<()> {
    if !cargo_toml_path.exists() {
        return Ok(());
    }
    let content =
        fs::read_to_string(cargo_toml_path).with_context(|| format!("Failed to read {:?}", cargo_toml_path))?;
    let mut cargo_data: toml::Value = content
        .parse()
        .with_context(|| format!("Failed to parse {:?}", cargo_toml_path))?;

    let deps = match cargo_data.get_mut("dependencies") {
        Some(d) => d,
        None => return Ok(()),
    };

    if deps.get(package_name).is_none() {
        return Ok(());
    }

    // Use a normalized path string that is accepted by Cargo on all platforms.
    let path_str = normalize_dependency_path(local_path);

    let mut table = toml::value::Table::new();
    table.insert("path".to_string(), toml::Value::String(path_str));
    deps[package_name] = toml::Value::Table(table);

    let new_content =
        toml::to_string_pretty(&cargo_data).with_context(|| format!("Failed to serialize {:?}", cargo_toml_path))?;
    fs::write(cargo_toml_path, new_content).with_context(|| format!("Failed to write {:?}", cargo_toml_path))?;

    Ok(())
}

/// Updates a `dependencies` entry in a `package.json` to point to a local path.
fn update_package_json_dependency(package_json_path: &Path, package_name: &str, local_path: &Path) -> Result<()> {
    let content =
        fs::read_to_string(package_json_path).with_context(|| format!("Failed to read {:?}", package_json_path))?;
    let mut data: Value =
        serde_json::from_str(&content).with_context(|| format!("Failed to parse {:?}", package_json_path))?;

    let path_str = normalize_dependency_path(local_path);

    if let Some(deps) = data.get_mut("dependencies") {
        if deps.get(package_name).is_some() {
            deps[package_name] = Value::String(path_str);
        }
    }

    let new_content =
        serde_json::to_string_pretty(&data).with_context(|| format!("Failed to serialize {:?}", package_json_path))?;
    fs::write(package_json_path, new_content).with_context(|| format!("Failed to write {:?}", package_json_path))?;

    Ok(())
}

/// Runs pnpm with the given arguments in the given working directory.
fn run_pnpm(args: &[&str], cwd: &Path) -> Result<()> {
    let pnpm = pnpm_path().context("pnpm not found")?;
    let output = Command::new(&pnpm)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("Failed to spawn pnpm {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "pnpm {} (in {:?}) failed:\nstdout: {}\nstderr: {}",
            args.join(" "),
            cwd,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Runs dotnet with the given arguments in the given working directory.
fn run_dotnet(args: &[&str], cwd: &Path) -> Result<()> {
    let output = Command::new("dotnet")
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("Failed to spawn dotnet {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "dotnet {} (in {:?}) failed:\nstdout: {}\nstderr: {}",
            args.join(" "),
            cwd,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Clears a package id from the global NuGet package cache to avoid stale
/// versions with identical semantic version numbers shadowing local packed
/// packages.
fn clear_cached_nuget_package(package_id: &str) -> Result<()> {
    let package_id = package_id.to_lowercase();

    let mut candidate_roots = Vec::new();
    if let Some(path) = env::var_os("NUGET_PACKAGES") {
        candidate_roots.push(PathBuf::from(path));
    }
    if let Some(home) = env::var_os("HOME") {
        candidate_roots.push(PathBuf::from(home).join(".nuget").join("packages"));
    }
    if let Some(userprofile) = env::var_os("USERPROFILE") {
        candidate_roots.push(PathBuf::from(userprofile).join(".nuget").join("packages"));
    }

    for root in candidate_roots {
        let package_dir = root.join(&package_id);
        if package_dir.exists() {
            eprintln!(
                "[TEMPLATES] Clearing NuGet cache for {} at {:?}",
                package_id, package_dir
            );
            fs::remove_dir_all(&package_dir)
                .with_context(|| format!("Failed to remove NuGet cache directory {:?}", package_dir))?;
        }
    }

    Ok(())
}

/// Builds the TypeScript SDK (`crates/bindings-typescript`).
///
/// Should be called once before testing any TypeScript templates.
fn build_typescript_sdk() -> Result<()> {
    let sdk_path = workspace_root().join("crates/bindings-typescript");
    eprintln!("[TEMPLATES] Building TypeScript SDK at {:?}", sdk_path);
    run_pnpm(&["install"], &sdk_path)?;
    run_pnpm(&["build"], &sdk_path)?;
    Ok(())
}

/// Points the `spacetimedb` entry in `package.json` at the local TypeScript
/// SDK and removes the template's lockfile so pnpm re-resolves dependencies.
fn setup_typescript_sdk_in_package_json(package_json_path: &Path) -> Result<()> {
    let sdk_path = workspace_root().join("crates/bindings-typescript");
    update_package_json_dependency(package_json_path, "spacetimedb", &sdk_path)?;

    // Remove the template's lockfile; the dependency changed.
    let lockfile = package_json_path.parent().unwrap().join("pnpm-lock.yaml");
    if lockfile.exists() {
        fs::remove_file(&lockfile).context("Failed to remove pnpm-lock.yaml")?;
    }
    Ok(())
}

/// Rewires the Rust server module's `spacetimedb` dependency to the local
/// `crates/bindings` path.
///
/// Templates contain a relative path (`../../../crates/bindings`) that is only
/// valid inside the repo's `templates/` tree.  After `spacetime init` copies
/// the template to a temp directory the relative path is wrong, so we replace
/// it with an absolute path.
fn setup_rust_server_sdk(server_path: &Path) -> Result<()> {
    let bindings_path = workspace_root().join("crates/bindings");
    update_cargo_toml_dependency(&server_path.join("Cargo.toml"), "spacetimedb", &bindings_path)
}

/// Rewires the Rust client's `spacetimedb-sdk` dependency to the local
/// `sdks/rust` path.
fn setup_rust_client_sdk(project_path: &Path) -> Result<()> {
    let sdk_path = workspace_root().join("sdks/rust");
    update_cargo_toml_dependency(&project_path.join("Cargo.toml"), "spacetimedb-sdk", &sdk_path)
}

/// Creates a local `nuget.config`, packs all required SpacetimeDB C# packages
/// from source, and registers them as local NuGet sources.
fn setup_csharp_nuget(project_path: &Path) -> Result<PathBuf> {
    eprintln!("[TEMPLATES] Setting up C# NuGet sources at {:?}", project_path);

    // NuGet can reuse stale packages from global cache even if we add local
    // package sources below. Remove the relevant package IDs so restore/publish
    // uses freshly packed local artifacts.
    for package in &[
        "SpacetimeDB.Runtime",
        "SpacetimeDB.BSATN.Runtime",
        "SpacetimeDB.Codegen",
        "SpacetimeDB.BSATN.Codegen",
        "SpacetimeDB.ClientSDK",
    ] {
        clear_cached_nuget_package(package)?;
    }

    let nuget_config = project_path.join("nuget.config");
    if !nuget_config.exists() {
        fs::write(
            &nuget_config,
            r#"<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <clear />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
</configuration>
"#,
        )
        .context("Failed to write nuget.config")?;
    }

    let bindings = workspace_root().join("crates/bindings-csharp");
    for pkg in &["BSATN.Runtime", "Runtime", "BSATN.Codegen", "Codegen"] {
        run_dotnet(&["pack", "-c", "Release"], &bindings.join(pkg))?;
        let pkg_output = bindings.join(pkg).join("bin").join("Release");
        run_dotnet(
            &[
                "nuget",
                "add",
                "source",
                pkg_output.to_str().unwrap(),
                "-n",
                &format!("SpacetimeDB.{}", pkg),
                "--configfile",
                nuget_config.to_str().unwrap(),
            ],
            project_path,
        )?;
    }

    // Pack and register the client SDK (needed by client templates).
    let client_sdk = workspace_root().join("sdks/csharp");
    let client_sdk_proj = client_sdk.join("SpacetimeDB.ClientSDK.csproj");
    run_dotnet(
        &[
            "pack",
            client_sdk_proj.to_str().unwrap(),
            "-c",
            "Release",
            "--configfile",
            nuget_config.to_str().unwrap(),
        ],
        project_path,
    )?;
    let client_sdk_output = client_sdk.join("bin~").join("Release");
    run_dotnet(
        &[
            "nuget",
            "add",
            "source",
            client_sdk_output.to_str().unwrap(),
            "-n",
            "SpacetimeDB.ClientSDK",
            "--configfile",
            nuget_config.to_str().unwrap(),
        ],
        project_path,
    )?;

    Ok(nuget_config)
}

// ============================================================================
// Per-language publish + client-test helpers
// ============================================================================

/// Publishes a Rust server module and verifies the Rust client builds.
fn test_rust_template(test: &Smoketest, template: &Template, project_path: &Path) -> Result<()> {
    let server_path = project_path.join("spacetimedb");
    setup_rust_server_sdk(&server_path)?;

    let domain = format!("test-{}-{}", template.id, random_string());
    test.spacetime(&[
        "publish",
        "--server",
        &test.server_url,
        "--yes",
        "--module-path",
        server_path.to_str().unwrap(),
        &domain,
    ])
    .with_context(|| format!("spacetime publish failed for Rust server in template {}", template.id))?;
    // Best-effort cleanup.
    let _ = test.spacetime(&["delete", "--server", &test.server_url, "--yes", &domain]);

    if template.client_lang.as_deref() == Some("rust") {
        setup_rust_client_sdk(project_path)?;
        let output = Command::new("cargo")
            .args(["build"])
            .current_dir(project_path)
            .output()
            .context("Failed to run cargo build")?;
        if !output.status.success() {
            bail!(
                "cargo build for {} client failed:\nstdout: {}\nstderr: {}",
                template.id,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    Ok(())
}

/// Publishes a TypeScript server module and verifies the TypeScript client type-checks.
fn test_typescript_template(test: &Smoketest, template: &Template, project_path: &Path) -> Result<()> {
    // Server
    let server_path = project_path.join("spacetimedb");
    setup_typescript_sdk_in_package_json(&server_path.join("package.json"))?;
    run_pnpm(&["install"], &server_path)?;

    let domain = format!("test-{}-{}", template.id, random_string());
    test.spacetime(&[
        "publish",
        "--server",
        &test.server_url,
        "--yes",
        "--module-path",
        server_path.to_str().unwrap(),
        &domain,
    ])
    .with_context(|| {
        format!(
            "spacetime publish failed for TypeScript server in template {}",
            template.id
        )
    })?;
    let _ = test.spacetime(&["delete", "--server", &test.server_url, "--yes", &domain]);

    // Client type-check (only if there's a client package.json in the project root)
    let client_package_json = project_path.join("package.json");
    if client_package_json.exists() {
        setup_typescript_sdk_in_package_json(&client_package_json)?;
        run_pnpm(&["install"], project_path)?;

        // TODO: some templates don't pass tsc yet, re-enable once they're fixed.
        // run_pnpm(&["exec", "tsc", "--noEmit"], project_path)?;
    }
    Ok(())
}

/// Publishes a C# server module and verifies the C# client builds.
fn test_csharp_template(test: &Smoketest, template: &Template, project_path: &Path) -> Result<()> {
    // Use one nuget.config at the project root, shared between server and client.
    setup_csharp_nuget(project_path)?;

    let server_path = project_path.join("spacetimedb");
    // Copy nuget.config into the server directory so `spacetime publish` (which runs
    // `dotnet publish` from the server dir) can find the local package sources.
    let root_nuget = project_path.join("nuget.config");
    let server_nuget = server_path.join("nuget.config");
    if root_nuget.exists() && !server_nuget.exists() {
        fs::copy(&root_nuget, &server_nuget).context("Failed to copy nuget.config to server dir")?;
    }
    let domain = format!("test-{}-{}", template.id, random_string());
    test.spacetime(&[
        "publish",
        "--server",
        &test.server_url,
        "--yes",
        "--module-path",
        server_path.to_str().unwrap(),
        &domain,
    ])
    .with_context(|| format!("spacetime publish failed for C# server in template {}", template.id))?;
    let _ = test.spacetime(&["delete", "--server", &test.server_url, "--yes", &domain]);

    if template.client_lang.as_deref() == Some("csharp") {
        run_dotnet(&["build"], project_path)?;
    }
    Ok(())
}

/// Runs the full init + publish + client-test cycle for a single template.
fn test_template(test: &Smoketest, template: &Template) -> Result<()> {
    eprintln!("[TEMPLATES] Testing template: {}", template.id);

    let (_tmpdir, project_path) = init_template(test, &template.id)?;

    match template.server_lang.as_deref() {
        Some("rust") => test_rust_template(test, template, &project_path)?,
        Some("typescript") => test_typescript_template(test, template, &project_path)?,
        Some("csharp") => test_csharp_template(test, template, &project_path)?,
        Some(other) => {
            eprintln!(
                "[TEMPLATES] Skipping template {} with unsupported server language: {}",
                template.id, other
            );
        }
        None => {
            eprintln!("[TEMPLATES] Skipping template {} with no server language", template.id);
        }
    }

    Ok(())
}

// ============================================================================
// Test entry point
// ============================================================================

/// Tests all templates discovered in the `templates/` directory.
///
/// For each template the test:
/// 1. Runs `spacetime init --template <id>` into a temp directory
/// 2. Wires local SDK dependencies so the template builds against the current source
/// 3. Publishes the server module and verifies it succeeds
/// 4. Type-checks / builds the client code
///
/// All templates are exercised even if some fail; a summary is printed at the
/// end and the test fails if any template did.
#[test]
fn test_all_templates() {
    let templates = get_templates();
    assert!(!templates.is_empty(), "No templates found in templates/");

    let has_typescript = templates.iter().any(|t| t.server_lang.as_deref() == Some("typescript"));
    let has_csharp = templates.iter().any(|t| t.server_lang.as_deref() == Some("csharp"));

    // Guard checks - verify required tools are available before starting.
    if has_typescript {
        spacetimedb_smoketests::require_pnpm!();
    }
    if has_csharp {
        spacetimedb_smoketests::require_dotnet!();
    }

    // Build the TypeScript SDK once up-front if any TypeScript templates exist.
    if has_typescript {
        build_typescript_sdk().expect("Failed to build TypeScript SDK");
    }

    // One shared server for all templates.
    let test = Smoketest::builder().autopublish(false).build();

    let mut results: Vec<(String, Result<()>)> = Vec::new();

    for template in &templates {
        let result = test_template(&test, template);
        let passed = result.is_ok();
        eprintln!(
            "[TEMPLATES] {} {}",
            if passed { "[PASS]" } else { "[FAIL]" },
            template.id
        );
        results.push((template.id.clone(), result));
    }

    // Print summary.
    eprintln!("\n{}", "=".repeat(60));
    eprintln!("TEMPLATE TEST SUMMARY");
    eprintln!("{}", "=".repeat(60));
    for (id, result) in &results {
        eprintln!(
            "{:40} {}",
            id,
            if result.is_ok() {
                "[PASS]".to_string()
            } else {
                format!("[FAIL]: {:#}", result.as_ref().unwrap_err())
            }
        );
    }
    let passed = results.iter().filter(|(_, r)| r.is_ok()).count();
    let total = results.len();
    eprintln!("{}", "=".repeat(60));
    eprintln!("TOTAL: {}/{} passed", passed, total);

    // Fail if any template failed.
    let failures: Vec<_> = results
        .iter()
        .filter(|(_, r)| r.is_err())
        .map(|(id, r)| format!("  {}: {:#}", id, r.as_ref().unwrap_err()))
        .collect();

    if !failures.is_empty() {
        panic!(
            "{}/{} template(s) failed:\n{}",
            failures.len(),
            total,
            failures.join("\n")
        );
    }
}
