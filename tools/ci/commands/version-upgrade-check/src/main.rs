#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use ci_version_upgrade_check::Args;
use clap::Parser;
use duct::cmd;

/// Verifies that the repository version upgrade tool still works.
#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    args: Args,
}

fn run_version_upgrade_check() -> Result<()> {
    cmd!(
        "cargo",
        "bump-versions",
        "123.456.789",
        "--rust-and-cli",
        "--csharp",
        "--typescript",
        "--cpp",
        "--accept-snapshots"
    )
    .run()?;
    Ok(())
}

fn main() -> Result<()> {
    Cli::parse();

    run_version_upgrade_check()
}
