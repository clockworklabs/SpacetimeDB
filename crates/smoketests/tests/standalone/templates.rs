use anyhow::{bail, Context, Result};
use regex::Regex;
use serde_json::Value;
use spacetimedb_guard::ensure_binaries_built;
use spacetimedb_smoketests::workspace_root;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

macro_rules! registered_template_ids {
    ($($test_name:ident => $template_id:literal),+ $(,)?) => {
        const REGISTERED_TEMPLATE_IDS: &[&str] = &[$($template_id),+];
    };
}

spacetimedb_smoketests::for_each_smoketest_template!(registered_template_ids);

fn init_template(template_id: &str) -> Result<(TempDir, PathBuf)> {
    let tmpdir = tempfile::tempdir().context("Failed to create temp dir")?;
    let project_name = format!("test-{template_id}");
    let project_path = tmpdir.path().join(&project_name);
    let config_path = tmpdir.path().join("config.toml");
    let output = Command::new(ensure_binaries_built())
        .arg("--config-path")
        .arg(&config_path)
        .args([
            "init",
            "--template",
            template_id,
            "--project-path",
            project_path.to_str().unwrap(),
            "--non-interactive",
            &project_name,
        ])
        .current_dir(tmpdir.path())
        .output()
        .with_context(|| format!("Failed to execute spacetime init --template {template_id}"))?;

    if !output.status.success() {
        bail!(
            "spacetime init --template {template_id} failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok((tmpdir, project_path))
}

fn fake_dotnet_path(dir: &Path, sdk_list_output: &str) -> Result<PathBuf> {
    let executable_name = if cfg!(windows) { "dotnet.exe" } else { "dotnet" };
    let dotnet_path = dir.join(executable_name);
    let echo_lines = sdk_list_output
        .lines()
        .map(|line| format!("echo {line}"))
        .collect::<Vec<_>>()
        .join(if cfg!(windows) { "\r\n" } else { "\n" });

    if cfg!(windows) {
        let source_path = dir.join("fake_dotnet.rs");
        fs::write(
            &source_path,
            format!(
                r#"fn main() {{
    if std::env::args().nth(1).as_deref() == Some("--list-sdks") {{
        print!("{{}}", {sdk_list_output:?});
        return;
    }}

    std::process::exit(1);
}}
"#
            ),
        )
        .with_context(|| format!("Failed to write fake dotnet source {source_path:?}"))?;

        let output = Command::new("rustc")
            .arg(&source_path)
            .arg("-o")
            .arg(&dotnet_path)
            .output()
            .context("Failed to spawn rustc for fake dotnet")?;
        if !output.status.success() {
            bail!(
                "rustc failed to compile fake dotnet:\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    } else {
        fs::write(
            &dotnet_path,
            format!("#!/usr/bin/env sh\nif [ \"$1\" = \"--list-sdks\" ]; then\n{echo_lines}\nexit 0\nfi\nexit 1\n"),
        )
        .with_context(|| format!("Failed to write fake dotnet executable {dotnet_path:?}"))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&dotnet_path)?.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&dotnet_path, permissions)?;
        }
    }

    Ok(dotnet_path)
}

fn init_basic_cs_with_fake_dotnet(sdk_list_output: &str) -> Result<(TempDir, PathBuf)> {
    let tmpdir = tempfile::tempdir().context("Failed to create temp dir")?;
    let fake_bin = tmpdir.path().join("bin");
    fs::create_dir(&fake_bin).context("Failed to create fake dotnet bin dir")?;
    fake_dotnet_path(&fake_bin, sdk_list_output)?;

    let current_path = env::var_os("PATH").unwrap_or_default();
    let test_path = env::join_paths(std::iter::once(fake_bin).chain(env::split_paths(&current_path)))
        .context("Failed to build test PATH")?;

    let project_name = "test-basic-cs-default-dotnet";
    let project_path = tmpdir.path().join(project_name);
    let config_path = tmpdir.path().join("config.toml");
    let output = Command::new(ensure_binaries_built())
        .arg("--config-path")
        .arg(&config_path)
        .args([
            "init",
            "--template",
            "basic-cs",
            "--project-path",
            project_path.to_str().unwrap(),
            "--non-interactive",
            project_name,
        ])
        .env("PATH", test_path)
        .current_dir(tmpdir.path())
        .output()
        .context("Failed to execute spacetime init")?;

    if !output.status.success() {
        bail!(
            "spacetime init with fake dotnet failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok((tmpdir, project_path))
}

fn assert_basic_cs_default_dotnet(sdk_list_output: &str, expected_major: u8) -> Result<()> {
    let (_tmpdir, project_path) = init_basic_cs_with_fake_dotnet(sdk_list_output)?;
    let server_path = project_path.join("spacetimedb");

    let global_json = fs::read_to_string(server_path.join("global.json")).context("Failed to read global.json")?;
    assert!(
        global_json.contains(&format!("\"version\": \"{expected_major}.0.100\"")),
        "global.json did not target .NET {expected_major}:\n{global_json}"
    );

    let csproj_path = find_csproj(&server_path)?;
    let csproj = fs::read_to_string(&csproj_path).with_context(|| format!("Failed to read {csproj_path:?}"))?;
    assert!(
        csproj.contains(&format!("<TargetFramework>net{expected_major}.0</TargetFramework>")),
        "{csproj_path:?} did not target net{expected_major}.0:\n{csproj}"
    );
    assert!(
        !csproj.contains("<TargetFrameworks>"),
        "{csproj_path:?} should use a single TargetFramework after init:\n{csproj}"
    );

    Ok(())
}

fn assert_major_minor_version(actual: &str, context: impl std::fmt::Display) -> Result<()> {
    let re = Regex::new(r"^\d+\.\d+$").unwrap();
    if !re.is_match(actual) {
        bail!("{context}: expected MAJOR.MINOR, got {actual}");
    }
    Ok(())
}

fn assert_major_minor_patch_wildcard(actual: &str, context: impl std::fmt::Display) -> Result<()> {
    let re = Regex::new(r"^\d+\.\d+\.\*$").unwrap();
    if !re.is_match(actual) {
        bail!("{context}: expected MAJOR.MINOR.*, got {actual}");
    }
    Ok(())
}

fn read_cargo_dependency_version(cargo_toml_path: &Path, package_name: &str) -> Result<String> {
    let content = fs::read_to_string(cargo_toml_path).with_context(|| format!("Failed to read {cargo_toml_path:?}"))?;
    let data: toml::Value = content
        .parse()
        .with_context(|| format!("Failed to parse {cargo_toml_path:?}"))?;
    let dep = data
        .get("dependencies")
        .and_then(|deps| deps.get(package_name))
        .with_context(|| format!("No dependency `{package_name}` found in {cargo_toml_path:?}"))?;
    match dep {
        toml::Value::String(version) => Ok(version.clone()),
        toml::Value::Table(table) => table
            .get("version")
            .and_then(|value| value.as_str())
            .map(String::from)
            .with_context(|| format!("Dependency `{package_name}` in {cargo_toml_path:?} has no version")),
        _ => bail!("Unsupported dependency `{package_name}` format in {cargo_toml_path:?}"),
    }
}

fn read_package_json_dependency_version(package_json_path: &Path, package_name: &str) -> Result<String> {
    let content =
        fs::read_to_string(package_json_path).with_context(|| format!("Failed to read {package_json_path:?}"))?;
    let data: Value =
        serde_json::from_str(&content).with_context(|| format!("Failed to parse {package_json_path:?}"))?;
    data.get("dependencies")
        .and_then(|deps| deps.get(package_name))
        .and_then(|value| value.as_str())
        .map(String::from)
        .with_context(|| format!("No dependency `{package_name}` found in {package_json_path:?}"))
}

fn read_csproj_package_reference_version(csproj_path: &Path, package_name: &str) -> Result<String> {
    let content = fs::read_to_string(csproj_path).with_context(|| format!("Failed to read {csproj_path:?}"))?;
    let root =
        xmltree::Element::parse(content.as_bytes()).with_context(|| format!("Failed to parse XML {csproj_path:?}"))?;

    root.children
        .iter()
        .filter_map(|node| match node {
            xmltree::XMLNode::Element(element) if element.name == "ItemGroup" => Some(element),
            _ => None,
        })
        .flat_map(|item_group| item_group.children.iter())
        .filter_map(|node| match node {
            xmltree::XMLNode::Element(element) if element.name == "PackageReference" => Some(element),
            _ => None,
        })
        .find(|package_ref| package_ref.attributes.get("Include").map(String::as_str) == Some(package_name))
        .and_then(|package_ref| package_ref.attributes.get("Version").cloned())
        .with_context(|| format!("No PackageReference `{package_name}` found in {csproj_path:?}"))
}

fn find_csproj(dir: &Path) -> Result<PathBuf> {
    fs::read_dir(dir)
        .with_context(|| format!("Failed to read {dir:?}"))?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "csproj"))
        .with_context(|| format!("No .csproj found in {dir:?}"))
}

fn read_spacetimedb_cpp_version(cmake_path: &Path) -> Result<String> {
    let content = fs::read_to_string(cmake_path).with_context(|| format!("Failed to read {cmake_path:?}"))?;
    let re = Regex::new(r#"set\(SPACETIMEDB_CPP_VERSION\s+"([^"]+)""#).unwrap();
    let captures = re
        .captures(&content)
        .with_context(|| format!("No SPACETIMEDB_CPP_VERSION found in {cmake_path:?}"))?;
    Ok(captures.get(1).unwrap().as_str().to_string())
}

fn discovered_template_ids() -> Vec<String> {
    let templates_dir = workspace_root().join("templates");
    let mut templates = fs::read_dir(&templates_dir)
        .unwrap_or_else(|error| panic!("Failed to read template directory {templates_dir:?}: {error}"))
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_dir()))
        .filter(|entry| entry.path().join(".template.json").exists())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    templates.sort();
    templates
}

/// Standalone-only: this initializes templates and inspects local dependency
/// manifests; a remote cluster is never contacted.
#[test]
fn test_basic_template_dependency_versions() -> Result<()> {
    let (_basic_cpp_tmpdir, basic_cpp_path) = init_template("basic-cpp")?;
    let cpp_server_version = read_spacetimedb_cpp_version(&basic_cpp_path.join("spacetimedb/CMakeLists.txt"))?;
    assert_major_minor_version(&cpp_server_version, "basic-cpp C++ server SPACETIMEDB_CPP_VERSION")?;
    let cpp_client_manifest = basic_cpp_path.join("Cargo.toml");
    if !cpp_client_manifest.exists() {
        bail!("basic-cpp expected Rust client manifest at {cpp_client_manifest:?}");
    }

    let (_basic_rs_tmpdir, basic_rs_path) = init_template("basic-rs")?;
    let rs_server_version =
        read_cargo_dependency_version(&basic_rs_path.join("spacetimedb/Cargo.toml"), "spacetimedb")?;
    assert_major_minor_patch_wildcard(&rs_server_version, "basic-rs Rust server spacetimedb")?;
    let rs_client_version = read_cargo_dependency_version(&basic_rs_path.join("Cargo.toml"), "spacetimedb-sdk")?;
    assert_major_minor_patch_wildcard(&rs_client_version, "basic-rs Rust client spacetimedb-sdk")?;

    let (_basic_ts_tmpdir, basic_ts_path) = init_template("basic-ts")?;
    let ts_server_version =
        read_package_json_dependency_version(&basic_ts_path.join("spacetimedb/package.json"), "spacetimedb")?;
    assert_major_minor_patch_wildcard(&ts_server_version, "basic-ts TypeScript server spacetimedb")?;
    let ts_client_version = read_package_json_dependency_version(&basic_ts_path.join("package.json"), "spacetimedb")?;
    assert_major_minor_patch_wildcard(&ts_client_version, "basic-ts TypeScript client spacetimedb")?;

    let (_basic_cs_tmpdir, basic_cs_path) = init_template("basic-cs")?;
    let cs_server_project = find_csproj(&basic_cs_path.join("spacetimedb"))?;
    let cs_server_version = read_csproj_package_reference_version(&cs_server_project, "SpacetimeDB.Runtime")?;
    assert_major_minor_patch_wildcard(&cs_server_version, "basic-cs C# server SpacetimeDB.Runtime")?;
    let cs_client_version =
        read_csproj_package_reference_version(&basic_cs_path.join("client.csproj"), "SpacetimeDB.ClientSDK")?;
    assert_major_minor_patch_wildcard(&cs_client_version, "basic-cs C# client SpacetimeDB.ClientSDK")?;

    Ok(())
}

/// Standalone-only: this uses a fake local `dotnet` executable to validate CLI
/// project generation and never contacts a server.
#[test]
fn test_basic_cs_init_default_dotnet_selection() -> Result<()> {
    assert_basic_cs_default_dotnet("8.0.416 [/usr/share/dotnet/sdk]", 8)?;
    assert_basic_cs_default_dotnet("10.0.100 [/usr/share/dotnet/sdk]", 10)?;
    assert_basic_cs_default_dotnet("8.0.416 [/usr/share/dotnet/sdk]\n10.0.100 [/usr/share/dotnet/sdk]", 10)?;
    assert_basic_cs_default_dotnet("", 10)?;
    Ok(())
}

/// Standalone-only: this compares local template metadata with the test
/// registry and never contacts a server.
#[test]
fn test_template_registry_matches_discovered_templates() {
    let registered = REGISTERED_TEMPLATE_IDS
        .iter()
        .map(|template_id| (*template_id).to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        discovered_template_ids(),
        registered,
        "Every discovered template must have its own nextest-visible test"
    );
}
