#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

pub fn ensure_repo_root() -> Result<()> {
    if !Path::new("Cargo.toml").exists() {
        bail!("You must execute this command from the SpacetimeDB repository root (where Cargo.toml is located)");
    }
    Ok(())
}

pub fn check_diff(subdir: &Path) -> Result<bool> {
    let pattern = r"^// This was generated using spacetimedb cli version.*";
    let status = Command::new("git")
        .args(["diff", "--exit-code"])
        .arg(format!("--ignore-matching-lines={pattern}"))
        .arg("--")
        .arg(subdir)
        .status()
        .with_context(|| format!("failed to spawn `git diff` for {}", subdir.display()))?;

    Ok(status.success())
}

pub fn check_diff_or_bail(subdir: &Path) -> Result<()> {
    if !check_diff(subdir)? {
        bail!("{} is dirty.", subdir.display());
    }
    Ok(())
}
