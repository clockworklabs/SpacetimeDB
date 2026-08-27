#![allow(clippy::disallowed_macros)]

use anyhow::{ensure, Result};
use ci_common::cargo;
use clap::Parser;
use duct::cmd;
use std::{env, path::PathBuf};

/// Checks that a module builds with the latest compatible dependencies.
#[derive(Parser)]
struct Cli {
    /// Do not build the CLI; use the binary selected by SPACETIME_BIN.
    #[arg(long)]
    no_build: bool,
}

fn target_dir() -> PathBuf {
    match env::var_os("CARGO_TARGET_DIR").map(PathBuf::from) {
        Some(path) if path.is_absolute() => path,
        Some(path) => ci_common::repo_root().join(path),
        None => ci_common::repo_root().join("target"),
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Build the CLI before updating the lockfile so a newly published incompatible dependency
    // cannot prevent us from exercising the fresh module dependency graph.
    let cli_path = if cli.no_build {
        ci_common::require_spacetime_bin()?
    } else {
        ensure!(
            env::var_os("SPACETIME_BIN").is_none(),
            "SPACETIME_BIN requires --no-build"
        );
        cargo(["build", "-vv", "-p", "spacetimedb-cli"]).run()?;
        target_dir()
            .join("debug/spacetimedb-cli")
            .with_extension(env::consts::EXE_EXTENSION)
    };

    // A fresh module gets the latest versions permitted by its dependency constraints rather
    // than the exact versions pinned in this repository's committed lockfile.
    cargo(["update", "-vv"]).run()?;
    cmd!(cli_path, "build", "--module-path", "modules/module-test").run()?;

    Ok(())
}
