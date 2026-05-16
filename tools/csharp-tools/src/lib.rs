#![allow(clippy::disallowed_macros)]

use anyhow::{Context, Result};
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

pub fn write_nuget_configs(target_dirs: &[PathBuf], spacetimedb_repo_path: Option<&Path>, quiet: bool) -> Result<()> {
    let spacetimedb_repo_path = match spacetimedb_repo_path {
        Some(path) => canonicalize_existing(path)?,
        None => workspace_dir(),
    };

    let config_contents = render_nuget_config(
        &spacetimedb_repo_path.join("crates/bindings-csharp/BSATN.Runtime/bin/Release"),
        &spacetimedb_repo_path.join("crates/bindings-csharp/Runtime/bin/Release"),
    );

    for target_dir in target_dirs {
        let target_dir = canonicalize_existing(target_dir)?;
        let config_path = target_dir.join("NuGet.Config");
        fs::write(&config_path, &config_contents)
            .with_context(|| format!("failed to write {}", config_path.display()))?;

        if !quiet {
            println!("Wrote {} contents:", config_path.display());
            print!("{}", fs::read_to_string(&config_path)?);
        }
    }

    Ok(())
}
