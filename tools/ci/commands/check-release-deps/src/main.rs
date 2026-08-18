#![allow(clippy::disallowed_macros)]

use anyhow::{Context, Result};
use clap::Parser;
use rollback_coordination::{earliest_rollback_point, Gh};
use std::collections::BTreeSet;
use std::fs;

#[derive(Parser)]
#[command(
    about = "Checks the `Rollback safety impact` section and verifies that every referenced PR has been released."
)]
struct Args {
    #[arg(long)]
    repo: String,
    #[arg(long = "allowed-repo", required = true)]
    allowed_repos: Vec<String>,
    #[arg(long)]
    pr_number: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let template = fs::read_to_string(".github/pull_request_template.md")
        .context("failed to read .github/pull_request_template.md")?;
    let allowed_repos = args.allowed_repos.into_iter().collect::<BTreeSet<_>>();
    let point = earliest_rollback_point(&Gh, &args.repo, &allowed_repos, Some(&template), &[args.pr_number])?;
    match point {
        Some(release) => println!("Earliest rollback point: {}", release.tag),
        None => println!("No PR mentions found, so trivially succeeding."),
    }
    Ok(())
}
