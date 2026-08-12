#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use clap::Parser;
use lookup_pr_release::{lookup, Gh};

#[derive(Parser)]
#[command(about = "Finds the earliest Git tag containing a pull request's squash commit.")]
struct Args {
    #[arg(long)]
    repo: String,
    /// Print PR, tag, and commit-range lookup progress.
    #[arg(long)]
    verbose: bool,
    pr_number: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let github = Gh;
    match lookup(&github, &args.repo, args.pr_number, args.verbose)? {
        Some(release) => {
            println!(
                "{}#{} was released in {} ({})",
                args.repo, args.pr_number, release.tag, release.created_at
            );
            Ok(())
        }
        None => bail!("{}#{} has not been released", args.repo, args.pr_number),
    }
}
