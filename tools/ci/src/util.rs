#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn ensure_repo_root() -> Result<()> {
    if !Path::new("Cargo.toml").exists() {
        bail!("You must execute this command from the SpacetimeDB repository root (where Cargo.toml is located)");
    }
    Ok(())
}

pub fn has_git_diff(subdir: &Path) -> Result<bool> {
    let pattern = r"^// This was generated using spacetimedb cli version.*";
    let (git_dir, pathspec) = git_dir_and_pathspec(subdir)?;
    let status = Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["diff", "--exit-code"])
        .arg(format!("--ignore-matching-lines={pattern}"))
        .arg("--")
        .arg(&pathspec)
        .status()
        .with_context(|| format!("failed to spawn `git diff` for {}", subdir.display()))?;

    Ok(!status.success())
}

pub fn bail_if_diff(subdir: &Path) -> Result<()> {
    if has_git_diff(subdir)? {
        bail!("{} is dirty.", subdir.display());
    }
    Ok(())
}

fn git_dir_and_pathspec(path: &Path) -> Result<(PathBuf, PathBuf)> {
    if path.is_dir() {
        Ok((path.to_path_buf(), PathBuf::from(".")))
    } else {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .with_context(|| format!("{} has no file name", path.display()))?;
        Ok((parent.to_path_buf(), PathBuf::from(file_name)))
    }
}
