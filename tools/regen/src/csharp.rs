#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use duct::cmd;
use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const BSATN_PACKAGE_ID: &str = "spacetimedb.bsatn.runtime";
const GODOTSHARP_ASSET_ID: &str = "GodotSharp";
const GODOTSHARP_DLL_FILE_NAME: &str = "GodotSharp.dll";
const GODOTSHARP_PACKAGE_ID: &str = "godotsharp";
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

fn clear_godot_intermediate_outputs() -> Result<()> {
    let godot_obj_dir = sdk_dir().join("obj~/godot");
    if godot_obj_dir.exists() {
        fs::remove_dir_all(&godot_obj_dir)?;
    }
    Ok(())
}

fn restored_package<'a>(assets: &'a Value, package_id: &str) -> Option<(&'a str, &'a Value)> {
    let libraries = assets.get("libraries")?.as_object()?;

    libraries.iter().find_map(|(key, package)| {
        let (id, version) = key.split_once('/')?;
        id.eq_ignore_ascii_case(package_id).then_some((version, package))
    })
}

fn restored_lib_package_file<'a>(package: &'a Value, file_name: &str) -> Option<&'a str> {
    package.get("files")?.as_array()?.iter().find_map(|file| {
        let path = file.as_str()?;
        let actual_file_name = path.rsplit('/').next()?;
        (path.starts_with("lib/") && actual_file_name == file_name).then_some(path)
    })
}

fn verify_godotsharp_restore() -> Result<()> {
    let sdk = sdk_dir();
    let assets_path = sdk.join("obj~/godot/project.assets.json");
    let assets =
        fs::read_to_string(&assets_path).with_context(|| format!("Failed to read {}", assets_path.display()))?;
    let assets: Value =
        serde_json::from_str(&assets).with_context(|| format!("Failed to parse {}", assets_path.display()))?;

    let Some((godotsharp_version, godotsharp_package)) = restored_package(&assets, GODOTSHARP_ASSET_ID) else {
        bail!(
            "Godot restore output {} does not contain {}; refusing to pack with --no-restore",
            assets_path.display(),
            GODOTSHARP_ASSET_ID
        );
    };

    let Some(godotsharp_dll_relative_path) = restored_lib_package_file(godotsharp_package, GODOTSHARP_DLL_FILE_NAME)
    else {
        bail!(
            "Godot restore output {} does not reference a lib package file named {}; refusing to pack with --no-restore",
            assets_path.display(),
            GODOTSHARP_DLL_FILE_NAME
        );
    };

    let godotsharp_dll = sdk
        .join("packages")
        .join(GODOTSHARP_PACKAGE_ID)
        .join(godotsharp_version)
        .join(godotsharp_dll_relative_path);

    if !godotsharp_dll.exists() {
        bail!(
            "Godot restore referenced {}, but the package DLL is missing at {}; refusing to pack with --no-restore",
            GODOTSHARP_ASSET_ID,
            godotsharp_dll.display()
        );
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
    clear_restored_package_dirs(GODOTSHARP_PACKAGE_ID)?;
    clear_restored_package_dirs(RUNTIME_PACKAGE_ID)?;
    clear_godot_intermediate_outputs()?;

    cmd!(
        "dotnet",
        "restore",
        "SpacetimeDB.ClientSDK.csproj",
        "--configfile",
        path_arg(&nuget_config_path),
    )
    .dir(&sdk)
    .run()?;

    cmd!(
        "dotnet",
        "restore",
        "SpacetimeDB.ClientSDK.Godot.csproj",
        "--configfile",
        path_arg(&nuget_config_path),
        // TODO: It should be possible to put this in Directory.Build.props, but it caused CI failures when we did.
        "-p:BaseOutputPath=bin~/",
        "-p:BaseIntermediateOutputPath=obj~/godot/",
        "-p:MSBuildProjectExtensionsPath=obj~/godot/",
    )
    .dir(&sdk)
    .run()?;
    verify_godotsharp_restore()?;

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

    cmd!(
        "dotnet",
        "pack",
        "SpacetimeDB.ClientSDK.Godot.csproj",
        "-c",
        "Release",
        "--no-restore",
        "-p:BaseOutputPath=bin~/",
        // TODO: It should be possible to put this in Directory.Build.props, but it caused CI failures when we did.
        "-p:BaseIntermediateOutputPath=obj~/godot/",
        "-p:MSBuildProjectExtensionsPath=obj~/godot/"
    )
    .dir(&sdk)
    .run()?;

    Ok(())
}
