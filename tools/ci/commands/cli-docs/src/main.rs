#![allow(clippy::disallowed_macros)]
use anyhow::Result;
use ci_cli_docs::Args;
use ci_common::pnpm;
use clap::Parser;
use duct::cmd;
use std::env;

#[derive(Parser)]
#[command(about = "Generates CLI documentation and checks for changes")]
struct Cli {
    #[command(flatten)]
    args: Args,
}

fn main() -> Result<()> {
    let Args { spacetime_path } = Cli::parse().args;
    if let Some(path) = spacetime_path {
        env::set_current_dir(path).ok();
    }
    let current_dir = env::current_dir().expect("No current directory!");
    let dir_name = current_dir.file_name().expect("No current directory!");
    if dir_name != "SpacetimeDB" && dir_name != "public" {
        anyhow::bail!("You must execute this binary from inside of the SpacetimeDB directory, or use --spacetime-path");
    }

    pnpm(["install", "--recursive"]).run()?;
    pnpm(["generate-cli-docs"]).dir("docs").run()?;
    let out = cmd!("git", "status", "--porcelain", "--", "docs").read()?;
    if out.is_empty() {
        println!("No docs changes detected");
    } else {
        anyhow::bail!("CLI docs are out of date:\n{out}");
    }

    Ok(())
}
