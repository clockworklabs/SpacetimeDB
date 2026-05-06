#![allow(clippy::disallowed_macros)]

use anyhow::{Context, Result};
use duct::cmd;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("tools/csharp-tools should be two levels below the workspace root")
        .to_path_buf()
}

pub fn sdk_dir() -> PathBuf {
    workspace_dir().join("sdks/csharp")
}

fn cli_manifest() -> PathBuf {
    workspace_dir().join("crates/cli/Cargo.toml")
}

fn standalone_manifest() -> PathBuf {
    workspace_dir().join("crates/standalone/Cargo.toml")
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn canonicalize_existing(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("failed to canonicalize {}", path.display()))
}

fn render_nuget_config(bsatn_source: &Path, runtime_source: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <clear />
    <!-- Experimental NuGet feed for Microsoft.DotNet.ILCompiler.LLVM packages -->
    <add key="dotnet-experimental" value="https://pkgs.dev.azure.com/dnceng/public/_packaging/dotnet-experimental/nuget/v3/index.json" />
    <!-- Local NuGet repositories -->
    <add key="Local SpacetimeDB.BSATN.Runtime" value="{}" />
    <!-- We need to override the module runtime as well because the examples use it -->
    <add key="Local SpacetimeDB.Runtime" value="{}" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
  <packageSourceMapping>
    <!-- Ensure that SpacetimeDB.BSATN.Runtime is used from the local folder. -->
    <!-- Otherwise we risk an outdated version being quietly pulled from NuGet for testing. -->
    <packageSource key="Local SpacetimeDB.BSATN.Runtime">
      <package pattern="SpacetimeDB.BSATN.Runtime" />
    </packageSource>
    <packageSource key="Local SpacetimeDB.Runtime">
      <package pattern="SpacetimeDB.Runtime" />
    </packageSource>
    <!-- Experimental packages for NativeAOT-LLVM compilation -->
    <packageSource key="dotnet-experimental">
      <package pattern="Microsoft.DotNet.ILCompiler.LLVM" />
      <package pattern="runtime.*" />
    </packageSource>
    <!-- Fallback for other packages (e.g. test deps). -->
    <packageSource key="nuget.org">
      <package pattern="*" />
    </packageSource>
  </packageSourceMapping>
</configuration>
"#,
        bsatn_source.display(),
        runtime_source.display(),
    )
}

pub fn write_persistent_nuget_configs(spacetimedb_repo_path: Option<&Path>) -> Result<()> {
    let spacetimedb_repo_path = match spacetimedb_repo_path {
        Some(path) => canonicalize_existing(path)?,
        None => workspace_dir(),
    };

    let sdk_config = sdk_dir().join("NuGet.Config");
    let sdk_config_contents = render_nuget_config(
        &spacetimedb_repo_path.join("crates/bindings-csharp/BSATN.Runtime/bin/Release"),
        &spacetimedb_repo_path.join("crates/bindings-csharp/Runtime/bin/Release"),
    );
    fs::write(&sdk_config, sdk_config_contents).with_context(|| format!("failed to write {}", sdk_config.display()))?;

    let repo_config = spacetimedb_repo_path.join("NuGet.Config");
    let repo_config_contents = render_nuget_config(
        Path::new("crates/bindings-csharp/BSATN.Runtime/bin/Release"),
        Path::new("crates/bindings-csharp/Runtime/bin/Release"),
    );
    fs::write(&repo_config, repo_config_contents)
        .with_context(|| format!("failed to write {}", repo_config.display()))?;

    println!("Wrote {} contents:", sdk_config.display());
    print!("{}", fs::read_to_string(&sdk_config)?);

    Ok(())
}

fn remove_obj_tilde_children(parent: &Path) -> Result<()> {
    if !parent.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let obj_tilde = entry.path().join("obj~");
            if obj_tilde.exists() {
                fs::remove_dir_all(&obj_tilde).with_context(|| format!("failed to remove {}", obj_tilde.display()))?;
            }
        }
    }
    Ok(())
}

fn clean_procedure_obj_tilde_dirs() -> Result<()> {
    let procedure_client = sdk_dir().join("examples~/regression-tests/procedure-client");
    println!("Cleanup obj~ folders generated in {}", procedure_client.display());
    remove_obj_tilde_children(&procedure_client)?;
    remove_obj_tilde_children(&procedure_client.join("module_bindings"))?;
    Ok(())
}

pub fn run_regression_tests() -> Result<()> {
    let sdk = sdk_dir();
    let workspace = workspace_dir();
    let server_url = env::var("SPACETIMEDB_SERVER_URL").unwrap_or_else(|_| "local".to_string());

    cmd!("cargo", "regen", "csharp", "regression-tests").run()?;

    cmd!("cargo", "build", "--manifest-path", path_arg(&standalone_manifest())).run()?;

    cmd!(
        "cargo",
        "run",
        "--manifest-path",
        path_arg(&cli_manifest()),
        "--",
        "publish",
        "-c",
        "-y",
        "--server",
        &server_url,
        "-p",
        path_arg(&sdk.join("examples~/regression-tests/server")),
        "btree-repro",
    )
    .run()?;
    cmd!(
        "cargo",
        "run",
        "--manifest-path",
        path_arg(&cli_manifest()),
        "--",
        "publish",
        "-c",
        "-y",
        "--server",
        &server_url,
        "-p",
        path_arg(&sdk.join("examples~/regression-tests/republishing/server-initial")),
        "republish-test",
    )
    .run()?;
    cmd!(
        "cargo",
        "run",
        "--manifest-path",
        path_arg(&cli_manifest()),
        "--",
        "call",
        "--server",
        &server_url,
        "republish-test",
        "insert",
        "1",
    )
    .run()?;
    cmd!(
        "cargo",
        "run",
        "--manifest-path",
        path_arg(&cli_manifest()),
        "--",
        "publish",
        "--server",
        &server_url,
        "-p",
        path_arg(&sdk.join("examples~/regression-tests/republishing/server-republish")),
        "--break-clients",
        "republish-test",
    )
    .run()?;
    cmd!(
        "cargo",
        "run",
        "--manifest-path",
        path_arg(&cli_manifest()),
        "--",
        "call",
        "--server",
        &server_url,
        "republish-test",
        "insert",
        "2",
    )
    .run()?;

    clean_procedure_obj_tilde_dirs()?;

    cmd!(
        "cargo",
        "run",
        "--manifest-path",
        path_arg(&cli_manifest()),
        "--",
        "publish",
        "-c",
        "-y",
        "--server",
        &server_url,
        "-p",
        path_arg(&workspace.join("modules/sdk-test-procedure")),
        "procedure-tests",
    )
    .run()?;

    cmd!("dotnet", "run", "-c", "Debug")
        .dir(sdk.join("examples~/regression-tests/client"))
        .run()?;
    cmd!("dotnet", "run", "-c", "Debug")
        .dir(sdk.join("examples~/regression-tests/republishing/client"))
        .run()?;
    cmd!("dotnet", "run", "-c", "Debug")
        .dir(sdk.join("examples~/regression-tests/procedure-client"))
        .run()?;

    Ok(())
}
