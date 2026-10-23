#![allow(clippy::disallowed_macros)]

use anyhow::{Context, Result};
use clap::Parser;
use rollback_coordination::{earliest_rollback_point, Gh};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    about = "Checks the `Rollback safety impact` section and verifies that every referenced PR has been released."
)]
struct Args {
    #[arg(long)]
    current_repo: PathBuf,
    /// Paths to local clones of the repos that are allowed to be considered as release dependencies in the rollback safety PR section
    #[arg(long = "allowed-reference-repo", required = true)]
    allowed_reference_repos: Vec<PathBuf>,
    #[arg(long)]
    pr_number: u64,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let args = Args::parse();
    let template = fs::read_to_string(args.current_repo.join(".github/pull_request_template.md"))
        .context("failed to read .github/pull_request_template.md")?;
    let allowed_reference_repos = args
        .allowed_reference_repos
        .iter()
        .map(PathBuf::as_path)
        .collect::<Vec<_>>();
    let point = earliest_rollback_point(
        &Gh,
        &args.current_repo,
        &allowed_reference_repos,
        Some(&template),
        false,
        &[args.pr_number],
    )?;
    match point {
        Some(release) => println!("All mentioned PRs have been released. Earliest rollback point: {release}"),
        None => println!("No PR mentions found, so trivially succeeding."),
    }
    Ok(())
}
