#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Result};
use clap::Parser;
use lookup_pr_release::{lookup_release_for, Gh};

#[derive(Parser)]
#[command(about = "Finds the earliest Git tag containing a pull request's squash commit.")]
struct Args {
    #[arg(long)]
    repo: String,
    pr_number: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let github = Gh;
    match lookup_release_for(&github, &args.repo, args.pr_number)? {
        Some(release) => {
            println!(
                "{}#{} was released in {} ({})",
                args.repo, args.pr_number, release.tag, release.published_at
            );
            Ok(())
        }
        None => bail!("{}#{} has not been released", args.repo, args.pr_number),
    }
}
