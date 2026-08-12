#![allow(clippy::disallowed_macros)]

use anyhow::{bail, Context, Result};
use clap::Parser;
use lookup_pr_release::{lookup, references, validated_release_section, Gh, Github};
use serde::Deserialize;
use std::fs;

#[derive(Parser)]
#[command(about = "Checks that every PR in the `Must be released` section has been released.")]
struct Args {
    #[arg(long)]
    repo: String,
    #[arg(long)]
    pr_number: u64,
}

#[derive(Deserialize)]
struct PullRequest {
    body: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let github = Gh;
    let pr: PullRequest = github.get(&format!("repos/{}/pulls/{}", args.repo, args.pr_number))?;
    let body = pr.body.as_deref().unwrap_or_default();
    let template = fs::read_to_string(".github/pull_request_template.md")
        .context("failed to read .github/pull_request_template.md")?;
    let section =
        validated_release_section(body, &template).context("failed to validate the `Must be released` section")?;
    let dependencies = references(section, &args.repo)?;
    let mut unreleased = Vec::new();
    let mut errors = Vec::new();

    for dependency in dependencies {
        match lookup(&github, &dependency.repo, dependency.number, false) {
            Ok(Some(release)) => println!(
                "{}#{} was released in {}",
                dependency.repo, dependency.number, release.tag
            ),
            Ok(None) => unreleased.push(format!("{}#{}", dependency.repo, dependency.number)),
            Err(error) => errors.push(format!("{}#{}: {error:#}", dependency.repo, dependency.number)),
        }
    }

    if !unreleased.is_empty() || !errors.is_empty() {
        let mut message = String::new();
        if !unreleased.is_empty() {
            message.push_str("The following required PRs have not been released:\n");
            for dependency in unreleased {
                message.push_str(&format!("- {dependency}\n"));
            }
        }
        if !errors.is_empty() {
            message.push_str("Release lookup failed for:\n");
            for error in errors {
                message.push_str(&format!("- {error}\n"));
            }
        }
        bail!(message.trim_end().to_owned());
    }
    Ok(())
}
