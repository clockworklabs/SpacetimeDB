#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use ci_common::pnpm;
use clap::Parser;

/// Builds the docs site.
#[derive(Parser)]
struct Cli {}

fn main() -> Result<()> {
    Cli::parse();

    pnpm(["install"]).dir("docs").run()?;
    pnpm(["build"]).dir("docs").run()?;
    Ok(())
}
