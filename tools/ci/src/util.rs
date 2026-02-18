#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use clap::{CommandFactory, Parser, Subcommand};
use duct::cmd;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::{env, fs};

pub fn ensure_repo_root() -> Result<()> {
    if !Path::new("Cargo.toml").exists() {
        bail!("You must execute this command from the SpacetimeDB repository root (where Cargo.toml is located)");
    }
    Ok(())
}
