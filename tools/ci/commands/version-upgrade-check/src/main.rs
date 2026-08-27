#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use ci_common::cargo;
use clap::Parser;

/// Verifies that the repository version upgrade tool still works.
#[derive(Parser)]
struct Cli {}

fn run_version_upgrade_check() -> Result<()> {
    cargo([
        "bump-versions",
        "123.456.789",
        "--rust-and-cli",
        "--csharp",
        "--typescript",
        "--cpp",
        "--accept-snapshots",
    ])
    .run()?;
    Ok(())
}

fn main() -> Result<()> {
    Cli::parse();

    run_version_upgrade_check()
}
