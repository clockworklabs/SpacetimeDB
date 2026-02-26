#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use std::path::Path;

pub fn ensure_repo_root() -> Result<()> {
    if !Path::new("Cargo.toml").exists() {
        bail!("You must execute this command from the SpacetimeDB repository root (where Cargo.toml is located)");
    }
    Ok(())
}
