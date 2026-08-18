#![allow(clippy::disallowed_macros)]

use anyhow::{Context, Result};
use clap::Parser;
use lookup_pr_release::{earliest_rollback_point, Gh};
use std::collections::BTreeSet;
use std::fs;

#[derive(Parser)]
#[command(
    about = "Checks the `Rollback safety impact` section and verifies that every referenced PR has been released."
)]
struct Args {
    #[arg(long)]
    repo: String,
    #[arg(long)]
    pr_number: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let template = fs::read_to_string(".github/pull_request_template.md")
        .context("failed to read .github/pull_request_template.md")?;
    let allowed_repos = BTreeSet::from([args.repo.clone()]);
    let point = earliest_rollback_point(&Gh, &args.repo, &[args.pr_number], &allowed_repos, Some(&template))?;
    match point {
        Some(release) => println!("Earliest rollback point: {}", release.tag),
        None => println!("No PR mentions found, so trivially succeeding."),
    }
    Ok(())
}
