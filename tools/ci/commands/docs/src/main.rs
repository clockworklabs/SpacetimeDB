#![allow(clippy::disallowed_macros)]
use anyhow::{bail, Result};
use ci_common::{pnpm, repo_root};
use clap::Parser;

mod links;

/// Builds the docs site, then checks that links to it from elsewhere in the repository resolve.
#[derive(Parser)]
struct Cli {
    /// Check links against the existing `docs/build` instead of rebuilding the site.
    #[arg(long)]
    skip_build: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.skip_build {
        pnpm(["install"]).dir("docs").run()?;
        pnpm(["build"]).dir("docs").run()?;
    }

    let root = repo_root();
    let broken = links::check(&root, &root.join("docs/build"))?;
    if !broken.is_empty() {
        for link in &broken {
            eprintln!("{link}");
        }
        bail!(
            "{} links to https://spacetimedb.com/docs don't resolve against the docs build",
            broken.len()
        );
    }
    Ok(())
}
