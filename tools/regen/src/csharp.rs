#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use duct::cmd;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const BSATN_PACKAGE_ID: &str = "spacetimedb.bsatn.runtime";
const RUNTIME_PACKAGE_ID: &str = "spacetimedb.runtime";

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("tools/regen should be two levels below the workspace root")
        .to_path_buf()
}

fn sdk_dir() -> PathBuf {
    workspace_dir().join("sdks/csharp")
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
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

fn overlay_unity_meta_skeleton(pkg_id: &str) -> Result<()> {
    let sdk = sdk_dir();
    let skeleton_base = sdk.join("unity-meta-skeleton~");
    let skeleton_root = skeleton_base.join(pkg_id);
    if !skeleton_root.exists() {
        return Ok(());
    }

    let pkg_root = sdk.join("packages").join(pkg_id);
    if !pkg_root.exists() {
        return Ok(());
    }

    let pkg_root_meta = skeleton_base.join(format!("{pkg_id}.meta"));
    if pkg_root_meta.exists()
        && let Some(parent) = pkg_root.parent()
    {
        let pkg_meta_dst = parent.join(format!("{pkg_id}.meta"));
        fs::copy(&pkg_root_meta, &pkg_meta_dst)?;
    }

    let versioned_dir = match find_only_subdir(&pkg_root) {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("Skipping Unity meta overlay for {pkg_id}: could not locate restored version dir: {err}");
            return Ok(());
        }
    };

    let version_meta_template = skeleton_root.join("version.meta");
    if version_meta_template.exists()
        && let Some(parent) = versioned_dir.parent()
    {
        let version_name = versioned_dir
            .file_name()
            .expect("versioned directory should have a file name");
        let version_meta_dst = parent.join(format!("{}.meta", version_name.to_string_lossy()));
        fs::copy(&version_meta_template, &version_meta_dst)?;
    }

    copy_overlay_dir(&skeleton_root, &versioned_dir)
}

fn clear_restored_package_dirs(pkg_id: &str) -> Result<()> {
    let pkg_root = sdk_dir().join("packages").join(pkg_id);
    if pkg_root.exists() {
        fs::remove_dir_all(&pkg_root)?;
    }
    Ok(())
}

fn find_only_subdir(dir: &Path) -> Result<PathBuf> {
    let mut subdirs = vec![];

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            subdirs.push(entry.path());
        }
    }

    match subdirs.as_slice() {
        [] => bail!("Could not find a restored versioned directory under {}", dir.display()),
        [only] => Ok(only.clone()),
        _ => bail!(
            "Expected exactly one restored versioned directory under {}, found {}",
            dir.display(),
            subdirs.len()
        ),
    }
}

fn copy_overlay_dir(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        bail!("Skeleton directory does not exist: {}", src.display());
    }
    if !dst.exists() {
        bail!("Destination directory does not exist: {}", dst.display());
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            if dst_path.exists() {
                copy_overlay_dir(&src_path, &dst_path)?;
            }
        } else {
            if src_path.extension().is_some_and(|ext| ext == "meta") {
                let asset_path = dst_path
                    .parent()
                    .expect("dst_path should have a parent")
                    .join(dst_path.file_stem().expect(".meta file should have a file stem"));

                if asset_path.exists() {
                    fs::copy(&src_path, &dst_path)?;
                } else if dst_path.exists() {
                    fs::remove_file(&dst_path)?;
                }
                continue;
            }

            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

pub fn regen_dlls() -> Result<()> {
    let workspace = workspace_dir();
    let sdk = sdk_dir();

    cmd!(
        "dotnet",
        "pack",
        workspace.join("crates/bindings-csharp/BSATN.Runtime"),
        "-c",
        "Release"
    )
    .run()?;
    cmd!(
        "dotnet",
        "pack",
        workspace.join("crates/bindings-csharp/Runtime"),
        "-c",
        "Release"
    )
    .run()?;

    let nuget_config_dir = tempfile::tempdir()?;
    let nuget_config_path = nuget_config_dir.path().join("nuget.config");
    fs::write(
        &nuget_config_path,
        render_nuget_config(
            &workspace.join("crates/bindings-csharp/BSATN.Runtime/bin/Release"),
            &workspace.join("crates/bindings-csharp/Runtime/bin/Release"),
        ),
    )?;

    clear_restored_package_dirs(BSATN_PACKAGE_ID)?;
    clear_restored_package_dirs(RUNTIME_PACKAGE_ID)?;

    cmd!(
        "dotnet",
        "restore",
        "SpacetimeDB.ClientSDK.csproj",
        "--configfile",
        path_arg(&nuget_config_path),
    )
    .dir(&sdk)
    .run()?;

    overlay_unity_meta_skeleton(BSATN_PACKAGE_ID)?;
    overlay_unity_meta_skeleton(RUNTIME_PACKAGE_ID)?;

    cmd!(
        "dotnet",
        "pack",
        "SpacetimeDB.ClientSDK.csproj",
        "-c",
        "Release",
        "--no-restore"
    )
    .dir(&sdk)
    .run()?;

    Ok(())
}
