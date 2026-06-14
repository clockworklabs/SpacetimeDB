use std::collections::BTreeMap;
use std::env;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Subcommand};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::de::DeserializeOwned;
use serde::Deserialize;

const CLA_CONTEXT: &str = "license/cla";

#[derive(Subcommand)]
pub(crate) enum ClaAssistantCmd {
    /// Retries CLA Assistant if `license/cla` is the only remaining PR blocker.
    Retry(RetryArgs),
}

#[derive(Args)]
pub(crate) struct RetryArgs {
    /// Pull request number to check.
    #[arg(long)]
    pub(crate) pr_number: u64,

    /// Repository in `owner/name` form. Defaults to GITHUB_REPOSITORY.
    #[arg(long)]
    pub(crate) repo: Option<String>,
}

pub(crate) fn run(cmd: ClaAssistantCmd) -> Result<()> {
    match cmd {
        ClaAssistantCmd::Retry(args) => retry(args),
    }
}

fn retry(args: RetryArgs) -> Result<()> {
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
    let sha = pr.head.sha;
    let statuses = client.list_statuses(owner, repo, &sha)?;

    let latest_statuses = latest_status_by_context(statuses);
    if latest_statuses
        .get(CLA_CONTEXT)
        .is_some_and(|status| status.state == "success")
    {
        println!("PR #{pr_number} already has {CLA_CONTEXT}=success.");
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
    head: PullRequestRef,
}

#[derive(Deserialize)]
struct PullRequestRef {
    sha: String,
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
