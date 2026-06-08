use std::collections::BTreeMap;
use std::env;

use anyhow::{anyhow, bail, Context, Result};
use clap::Args;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::de::DeserializeOwned;
use serde::Deserialize;

const CLA_CONTEXT: &str = "license/cla";

#[derive(Args)]
pub(crate) struct RetryClaAssistantArgs {
    /// Pull request number to check.
    #[arg(long)]
    pub(crate) pr_number: u64,

    /// Repository in `owner/name` form. Defaults to GITHUB_REPOSITORY.
    #[arg(long)]
    pub(crate) repo: Option<String>,
}

pub(crate) fn run(args: RetryClaAssistantArgs) -> Result<()> {
    let repo = args
        .repo
        .or_else(|| env::var("GITHUB_REPOSITORY").ok())
        .context("repo is required via --repo or GITHUB_REPOSITORY")?;
    let (owner, repo_name) = repo
        .split_once('/')
        .ok_or_else(|| anyhow!("repo must be in owner/name form, got {repo:?}"))?;
    let token = env::var("GITHUB_TOKEN").context("GITHUB_TOKEN is required")?;
    let client = GithubClient::new(token)?;

    retry_for_pr(&client, owner, repo_name, args.pr_number)
}

fn retry_for_pr(client: &GithubClient, owner: &str, repo: &str, pr_number: u64) -> Result<()> {
    println!("Inspecting PR #{pr_number}");

    let pr: PullRequest = client.github_get(&format!("/repos/{owner}/{repo}/pulls/{pr_number}"))?;
    if pr.state != "open" {
        println!("PR #{pr_number} is {}; skipping.", pr.state);
        return Ok(());
    }
    if pr.draft {
        println!("PR #{pr_number} is draft; skipping.");
        return Ok(());
    }
    if pr.base.ref_name != "master" {
        println!("PR #{pr_number} targets {}, not master; skipping.", pr.base.ref_name);
        return Ok(());
    }

    let sha = pr.head.sha;
    let check_runs = client.list_check_runs(owner, repo, &sha)?;
    let statuses = client.list_statuses(owner, repo, &sha)?;

    let latest_statuses = latest_status_by_context(statuses);
    if latest_statuses
        .get(CLA_CONTEXT)
        .is_some_and(|status| status.state == "success")
    {
        println!("PR #{pr_number} already has {CLA_CONTEXT}=success.");
        return Ok(());
    }

    if check_runs.is_empty() {
        println!("PR #{pr_number} has no check runs yet; skipping.");
        return Ok(());
    }

    let blocking_check_runs: Vec<_> = check_runs.iter().filter(|run| !check_run_is_green(run)).collect();
    if !blocking_check_runs.is_empty() {
        println!("PR #{pr_number} still has non-green check runs:");
        for run in blocking_check_runs {
            println!(
                "- {}: status={}, conclusion={}",
                run.name,
                run.status,
                run.conclusion.as_deref().unwrap_or("none")
            );
        }
        return Ok(());
    }

    let blocking_statuses: Vec<_> = latest_statuses
        .values()
        .filter(|status| status.context != CLA_CONTEXT)
        .filter(|status| status.state != "success")
        .collect();
    if !blocking_statuses.is_empty() {
        println!("PR #{pr_number} still has non-green commit statuses:");
        for status in blocking_statuses {
            println!("- {}: {}", status.context, status.state);
        }
        return Ok(());
    }

    if let Some(cla_status) = latest_statuses.get(CLA_CONTEXT)
        && !matches!(cla_status.state.as_str(), "pending" | "failure" | "error")
    {
        println!(
            "PR #{pr_number} has unexpected {CLA_CONTEXT} state {}; skipping.",
            cla_status.state
        );
        return Ok(());
    }

    let reason = latest_statuses.get(CLA_CONTEXT).map_or_else(
        || format!("{CLA_CONTEXT} is missing"),
        |status| format!("{CLA_CONTEXT} is {}", status.state),
    );
    println!("Retrying CLA Assistant for PR #{pr_number}: {reason}");
    client.recheck_cla(owner, repo, pr_number)?;
    Ok(())
}

fn check_run_is_green(run: &CheckRun) -> bool {
    run.status == "completed" && matches!(run.conclusion.as_deref(), Some("success" | "skipped" | "neutral"))
}

fn latest_status_by_context(statuses: Vec<CommitStatus>) -> BTreeMap<String, CommitStatus> {
    // GitHub returns combined statuses newest-first, so keep the first context.
    let mut result = BTreeMap::new();
    for status in statuses {
        result.entry(status.context.clone()).or_insert(status);
    }
    result
}

struct GithubClient {
    http: Client,
}

impl GithubClient {
    fn new(token: String) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("clockworklabs-ci"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.github+json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).context("invalid GitHub token header")?,
        );
        Ok(Self {
            http: Client::builder().default_headers(headers).build()?,
        })
    }

    fn github_get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("https://api.github.com{path}");
        let response = self.http.get(&url).send()?;
        if !response.status().is_success() {
            bail!("GET {url} failed with HTTP {}", response.status());
        }
        Ok(response.json()?)
    }

    fn list_check_runs(&self, owner: &str, repo: &str, sha: &str) -> Result<Vec<CheckRun>> {
        let path = format!("/repos/{owner}/{repo}/commits/{sha}/check-runs");
        let response: CheckRunsResponse = self.github_get(&path)?;
        Ok(response.check_runs)
    }

    fn list_statuses(&self, owner: &str, repo: &str, sha: &str) -> Result<Vec<CommitStatus>> {
        let path = format!("/repos/{owner}/{repo}/commits/{sha}/status");
        let response: CombinedStatusResponse = self.github_get(&path)?;
        Ok(response.statuses)
    }

    fn recheck_cla(&self, owner: &str, repo: &str, pr_number: u64) -> Result<()> {
        let url = format!("https://cla-assistant.io/check/{owner}/{repo}?pullRequest={pr_number}");
        let response = self
            .http
            .get(&url)
            .header(ACCEPT, HeaderValue::from_static("text/plain, */*"))
            .send()?;
        println!("CLA Assistant recheck response: HTTP {}", response.status());
        if !response.status().is_success() {
            bail!("CLA Assistant recheck failed with HTTP {}", response.status());
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct PullRequest {
    state: String,
    draft: bool,
    head: PullRequestRef,
    base: PullRequestRef,
}

#[derive(Deserialize)]
struct PullRequestRef {
    sha: String,
    #[serde(rename = "ref")]
    ref_name: String,
}

#[derive(Deserialize)]
struct CheckRunsResponse {
    check_runs: Vec<CheckRun>,
}

#[derive(Deserialize)]
struct CheckRun {
    name: String,
    status: String,
    conclusion: Option<String>,
}

#[derive(Deserialize)]
struct CombinedStatusResponse {
    statuses: Vec<CommitStatus>,
}

#[derive(Clone, Deserialize)]
struct CommitStatus {
    context: String,
    state: String,
}
