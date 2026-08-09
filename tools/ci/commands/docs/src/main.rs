#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use ci_common::pnpm;
use ci_docs_build::Args;
use clap::Parser;

/// Builds the docs site.
#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    args: Args,
}

fn main() -> Result<()> {
    Cli::parse();

    pnpm(["install"]).dir("docs").run()?;
    pnpm(["build"]).dir("docs").run()?;
    Ok(())
}
