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

fn cli_manifest() -> PathBuf {
    workspace_dir().join("crates/cli/Cargo.toml")
}

fn standalone_manifest() -> PathBuf {
    workspace_dir().join("crates/standalone/Cargo.toml")
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn regen_regression_tests() -> Result<()> {
    let sdk = sdk_dir();
    let workspace = workspace_dir();

    cmd!("cargo", "build", "--manifest-path", path_arg(&standalone_manifest())).run()?;

    cmd!(
        "cargo",
        "run",
        "--manifest-path",
        path_arg(&cli_manifest()),
        "--",
        "generate",
        "-y",
        "-l",
        "csharp",
        "-o",
        path_arg(&sdk.join("examples~/regression-tests/client/module_bindings")),
        "--module-path",
        path_arg(&sdk.join("examples~/regression-tests/server")),
    )
    .run()?;
    cmd!(
        "cargo",
        "run",
        "--manifest-path",
        path_arg(&cli_manifest()),
        "--",
        "generate",
        "-y",
        "-l",
        "csharp",
        "-o",
        path_arg(&sdk.join("examples~/regression-tests/republishing/client/module_bindings")),
        "--module-path",
        path_arg(&sdk.join("examples~/regression-tests/republishing/server-republish")),
    )
    .run()?;
    cmd!(
        "cargo",
        "run",
        "--manifest-path",
        path_arg(&cli_manifest()),
        "--",
        "generate",
        "-y",
        "-l",
        "csharp",
        "-o",
        path_arg(&sdk.join("examples~/regression-tests/procedure-client/module_bindings")),
        "--module-path",
        path_arg(&workspace.join("modules/sdk-test-procedure")),
    )
    .run()?;

    Ok(())
}

pub fn regen_quickstart() -> Result<()> {
    let workspace = workspace_dir();

    cmd!("cargo", "build", "--manifest-path", path_arg(&standalone_manifest())).run()?;

    cmd!(
        "cargo",
        "run",
        "--manifest-path",
        path_arg(&cli_manifest()),
        "--",
        "generate",
        "-y",
        "-l",
        "csharp",
        "-o",
        path_arg(&workspace.join("templates/chat-console-cs/module_bindings")),
        "--module-path",
        path_arg(&workspace.join("templates/chat-console-cs/spacetimedb")),
    )
    .run()?;

    Ok(())
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
    let nuget_config_path = nuget_config_dir.path().join("NuGet.Config");
    cmd!(
        "cargo",
        "csharp",
        "write-nuget-config",
        path_arg(nuget_config_dir.path()),
        "--quiet",
    )
    .run()?;

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
