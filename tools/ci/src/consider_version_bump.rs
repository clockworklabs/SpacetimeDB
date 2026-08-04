use crate::util::ensure_repo_root;

use anyhow::{bail, Context, Result};
use clap::Parser;
use duct::cmd;
use regex::Regex;
use semver::Version;
use serde::Deserialize;
use std::env;
use std::fmt;
use std::fs;
use std::thread::sleep;
use std::time::Duration;

const SOURCE_REPO: &str = "clockworklabs/SpacetimeDB";
const MINOR_BUMP_LABEL: &str = "requires minor version bump";

/// Arguments for considering whether a master commit needs a version bump.
#[derive(Parser)]
pub struct ConsiderVersionBumpArgs {
    /// Commit SHA to evaluate. Defaults to GITHUB_SHA, then HEAD.
    #[arg(long)]
    commit: Option<String>,

    /// Repository owner for the downstream version-bump workflow.
    #[arg(long, default_value = "clockworklabs")]
    target_owner: String,

    /// Repository name for the downstream version-bump workflow.
    #[arg(long, default_value = "SpacetimeDBPrivate")]
    target_repo: String,

    /// Workflow file or workflow id to dispatch when a bump is needed.
    #[arg(long, default_value = "consider-version-bump.yml")]
    target_workflow: String,

    /// Ref to use when dispatching the downstream workflow.
    #[arg(long, default_value = "master")]
    target_ref: String,

    /// Number of attempts to find the PR associated with the commit.
    #[arg(long, default_value_t = 3)]
    max_attempts: u32,

    /// Delay between associated-PR lookup attempts.
    #[arg(long, default_value_t = 30)]
    retry_delay_seconds: u64,

    /// Print the intended action without dispatching a downstream workflow.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BumpKind {
    Patch,
    Minor,
}

impl fmt::Display for BumpKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BumpKind::Patch => f.write_str("patch"),
            BumpKind::Minor => f.write_str("minor"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct AssociatedPullRequest {
    number: u64,
}

#[derive(Debug, Deserialize)]
struct PullRequest {
    labels: Vec<Label>,
}

#[derive(Debug, Deserialize)]
struct Label {
    name: String,
}

pub fn run(args: ConsiderVersionBumpArgs) -> Result<()> {
    ensure_repo_root()?;

    let commit = resolve_commit(args.commit.as_deref())?;
    let associated_pr = associated_pr_for_commit(&commit, args.max_attempts, args.retry_delay_seconds)?;
    let bump_kind = match associated_pr {
        Some(pr_number) => bump_kind_for_pr(pr_number)?,
        None => BumpKind::Patch,
    };
    let current_version = read_workspace_version()?;
    let latest_release = latest_release_version()?;
    let required_version = required_version(&latest_release, bump_kind);

    println!("commit: {commit}");
    match associated_pr {
        Some(pr_number) => println!("associated_pr: #{pr_number}"),
        None => println!("associated_pr: none"),
    }
    println!("bump_kind: {bump_kind}");
    println!("latest_release_version: {latest_release}");
    println!("current_version: {current_version}");
    println!("required_version: {required_version}");

    if version_satisfies(&current_version, &required_version) {
        println!("result: no action needed");
        return Ok(());
    }

    println!("result: dispatching version bump workflow");
    dispatch_version_bump(
        &args,
        &VersionBumpDispatch {
            commit,
            associated_pr,
            bump_kind,
            latest_release,
            current_version,
            desired_version: required_version,
        },
    )
}

fn resolve_commit(commit: Option<&str>) -> Result<String> {
    if let Some(commit) = commit {
        return rev_parse_commit(commit);
    }
    if let Ok(commit) = env::var("GITHUB_SHA") {
        if !commit.is_empty() {
            return rev_parse_commit(&commit);
        }
    }
    rev_parse_commit("HEAD")
}

fn rev_parse_commit(commit: &str) -> Result<String> {
    Ok(cmd!("git", "rev-parse", commit)
        .read()
        .with_context(|| format!("failed to resolve commit {commit:?}"))?
        .trim()
        .to_owned())
}

fn associated_pr_for_commit(commit: &str, max_attempts: u32, retry_delay_seconds: u64) -> Result<Option<u64>> {
    let max_attempts = max_attempts.max(1);
    for attempt in 1..=max_attempts {
        let prs = associated_prs_for_commit(commit)?;
        match prs.as_slice() {
            [] if attempt < max_attempts => {
                sleep(Duration::from_secs(retry_delay_seconds));
            }
            [] => return Ok(None),
            [pr] => return Ok(Some(pr.number)),
            _ => {
                let numbers = prs
                    .iter()
                    .map(|pr| format!("#{}", pr.number))
                    .collect::<Vec<_>>()
                    .join(", ");
                bail!("commit {commit} is associated with multiple pull requests: {numbers}");
            }
        }
    }
    Ok(None)
}

fn associated_prs_for_commit(commit: &str) -> Result<Vec<AssociatedPullRequest>> {
    let output = cmd!(
        "gh",
        "api",
        "-H",
        "Accept: application/vnd.github+json",
        format!("repos/{SOURCE_REPO}/commits/{commit}/pulls")
    )
    .read()
    .with_context(|| format!("failed to query pull requests associated with commit {commit}"))?;
    serde_json::from_str(&output).context("failed to parse associated pull request response")
}

fn bump_kind_for_pr(pr_number: u64) -> Result<BumpKind> {
    let output = cmd!(
        "gh",
        "api",
        "-H",
        "Accept: application/vnd.github+json",
        format!("repos/{SOURCE_REPO}/pulls/{pr_number}")
    )
    .read()
    .with_context(|| format!("failed to query pull request #{pr_number}"))?;
    let pr: PullRequest = serde_json::from_str(&output).context("failed to parse pull request response")?;
    if pr.labels.iter().any(|label| label.name == MINOR_BUMP_LABEL) {
        Ok(BumpKind::Minor)
    } else {
        Ok(BumpKind::Patch)
    }
}

fn read_workspace_version() -> Result<Version> {
    let cargo_toml = fs::read_to_string("Cargo.toml").context("failed to read Cargo.toml")?;
    parse_workspace_version(&cargo_toml)
}

fn parse_workspace_version(cargo_toml: &str) -> Result<Version> {
    let workspace_package = cargo_toml
        .split_once("[workspace.package]")
        .map(|(_, after)| after)
        .context("Cargo.toml is missing [workspace.package]")?;
    let workspace_package = workspace_package
        .split("\n[")
        .next()
        .context("failed to isolate [workspace.package] section")?;
    let version_regex = Regex::new(r#"(?m)^\s*version\s*=\s*"([^"]+)""#)?;
    let version = version_regex
        .captures(workspace_package)
        .and_then(|captures| captures.get(1))
        .map(|m| m.as_str())
        .context("[workspace.package] is missing version")?;
    Version::parse(version).with_context(|| format!("failed to parse workspace version {version:?}"))
}

fn latest_release_version() -> Result<Version> {
    let tag = cmd!(
        "gh",
        "release",
        "view",
        "--repo",
        SOURCE_REPO,
        "--json",
        "tagName",
        "--jq",
        ".tagName"
    )
    .read()
    .context("failed to query latest release")?;
    parse_release_tag(tag.trim())
}

fn parse_release_tag(tag: &str) -> Result<Version> {
    let version = tag.strip_prefix('v').unwrap_or(tag);
    Version::parse(version).with_context(|| format!("failed to parse latest release tag {tag:?}"))
}

fn required_version(latest_release: &Version, bump_kind: BumpKind) -> Version {
    match bump_kind {
        BumpKind::Patch => Version::new(latest_release.major, latest_release.minor, latest_release.patch + 1),
        BumpKind::Minor => Version::new(latest_release.major, latest_release.minor + 1, 0),
    }
}

fn version_satisfies(current_version: &Version, required_version: &Version) -> bool {
    current_version >= required_version
}

struct VersionBumpDispatch {
    commit: String,
    associated_pr: Option<u64>,
    bump_kind: BumpKind,
    latest_release: Version,
    current_version: Version,
    desired_version: Version,
}

fn dispatch_version_bump(args: &ConsiderVersionBumpArgs, dispatch: &VersionBumpDispatch) -> Result<()> {
    let repo = format!("{}/{}", args.target_owner, args.target_repo);
    let mut gh_args = vec![
        "api".to_owned(),
        "-X".to_owned(),
        "POST".to_owned(),
        format!("repos/{repo}/actions/workflows/{}/dispatches", args.target_workflow),
        "-f".to_owned(),
        format!("ref={}", args.target_ref),
        "-f".to_owned(),
        format!("inputs[public_ref]={}", dispatch.commit),
        "-f".to_owned(),
        format!("inputs[bump_kind]={}", dispatch.bump_kind),
        "-f".to_owned(),
        format!("inputs[latest_release_version]={}", dispatch.latest_release),
        "-f".to_owned(),
        format!("inputs[current_version]={}", dispatch.current_version),
        "-f".to_owned(),
        format!("inputs[desired_version]={}", dispatch.desired_version),
    ];

    if let Some(pr_number) = dispatch.associated_pr {
        gh_args.push("-f".to_owned());
        gh_args.push(format!("inputs[public_pr_number]={pr_number}"));
    }

    println!("workflow: {repo}/{}@{}", args.target_workflow, args.target_ref);
    if args.dry_run {
        println!("dry_run: true");
        return Ok(());
    }

    cmd("gh", gh_args)
        .run()
        .with_context(|| format!("failed to dispatch {repo} workflow {}", args.target_workflow))
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_required_version_increments_patch() {
        let latest = Version::parse("2.8.0").unwrap();
        assert_eq!(
            required_version(&latest, BumpKind::Patch),
            Version::parse("2.8.1").unwrap()
        );
    }

    #[test]
    fn minor_required_version_increments_minor_and_resets_patch() {
        let latest = Version::parse("2.8.4").unwrap();
        assert_eq!(
            required_version(&latest, BumpKind::Minor),
            Version::parse("2.9.0").unwrap()
        );
    }

    #[test]
    fn current_version_satisfies_required_or_newer() {
        assert!(version_satisfies(
            &Version::parse("2.8.1").unwrap(),
            &Version::parse("2.8.1").unwrap()
        ));
        assert!(version_satisfies(
            &Version::parse("2.9.0").unwrap(),
            &Version::parse("2.8.1").unwrap()
        ));
        assert!(!version_satisfies(
            &Version::parse("2.8.0").unwrap(),
            &Version::parse("2.8.1").unwrap()
        ));
    }

    #[test]
    fn release_tags_may_start_with_v() {
        assert_eq!(parse_release_tag("v2.8.0").unwrap(), Version::parse("2.8.0").unwrap());
        assert_eq!(parse_release_tag("2.8.0").unwrap(), Version::parse("2.8.0").unwrap());
    }

    #[test]
    fn parses_workspace_package_version() {
        let cargo_toml = r#"
[package]
version = "0.1.0"

[workspace.package]
edition = "2021"
version = "2.8.0"

[workspace.dependencies]
semver = "1"
"#;
        assert_eq!(
            parse_workspace_version(cargo_toml).unwrap(),
            Version::parse("2.8.0").unwrap()
        );
    }
}
