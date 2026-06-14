use std::collections::BTreeMap;
use std::env;

use anyhow::{anyhow, bail, Context, Result};
use clap::{ArgGroup, Args, Subcommand};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const CLA_CONTEXT: &str = "license/cla";

#[derive(Subcommand)]
pub(crate) enum ClaAssistantCmd {
    /// Retries CLA Assistant if `license/cla` is the only remaining PR blocker.
    Retry(RetryArgs),

    /// Returns the `license/cla` status for a pull request or commit SHA.
    Status(StatusArgs),
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

#[derive(Args)]
#[command(group(
    ArgGroup::new("target")
        .required(true)
        .multiple(false)
        .args(["pr", "sha"]),
))]
pub(crate) struct StatusArgs {
    /// Pull request number whose head commit should be checked.
    #[arg(long)]
    pub(crate) pr: Option<u64>,

    /// Commit SHA to check.
    #[arg(long)]
    pub(crate) sha: Option<String>,

    /// Repository in `owner/name` form. Defaults to GITHUB_REPOSITORY.
    #[arg(long)]
    pub(crate) repo: Option<String>,
}

pub(crate) fn run(cmd: ClaAssistantCmd) -> Result<()> {
    match cmd {
        ClaAssistantCmd::Retry(args) => retry(args),
        ClaAssistantCmd::Status(args) => status(args),
    }
}

fn retry(args: RetryArgs) -> Result<()> {
    let repo = Repo::from_arg_or_env(args.repo)?;
    let token = env::var("GITHUB_TOKEN").context("GITHUB_TOKEN is required")?;
    let client = GithubClient::new(token)?;

    retry_for_pr(&client, &repo, args.pr_number)
}

fn status(args: StatusArgs) -> Result<()> {
    let repo = Repo::from_arg_or_env(args.repo)?;
    let token = env::var("GITHUB_TOKEN").context("GITHUB_TOKEN is required")?;
    let client = GithubClient::new(token)?;

    let sha = match (args.pr, args.sha) {
        (Some(pr_number), None) => client.pull_request(&repo, pr_number)?.head.sha,
        (None, Some(sha)) => sha,
        _ => unreachable!("clap requires exactly one of --pr or --sha"),
    };

    let statuses = client.list_statuses(&repo, &sha)?;
    let latest_statuses = latest_status_by_context(statuses);
    let output = latest_statuses.get(CLA_CONTEXT).map_or_else(
        || ClaStatusOutput::missing(sha.clone()),
        |status| ClaStatusOutput {
            sha: sha.clone(),
            state: Some(status.state.clone()),
            description: status.description.clone(),
            target_url: status.target_url.clone(),
        },
    );

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

fn retry_for_pr(client: &GithubClient, repo: &Repo, pr_number: u64) -> Result<()> {
    println!("Inspecting PR #{pr_number}");

    let pr = client.pull_request(repo, pr_number)?;
    let sha = pr.head.sha;
    let statuses = client.list_statuses(repo, &sha)?;

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
    client.recheck_cla(repo, pr_number)?;
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

struct Repo {
    owner: String,
    name: String,
}

impl Repo {
    fn from_arg_or_env(repo: Option<String>) -> Result<Self> {
        let repo = repo
            .or_else(|| env::var("GITHUB_REPOSITORY").ok())
            .context("repo is required via --repo or GITHUB_REPOSITORY")?;
        let (owner, name) = repo
            .split_once('/')
            .ok_or_else(|| anyhow!("repo must be in owner/name form, got {repo:?}"))?;
        Ok(Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }
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

    fn pull_request(&self, repo: &Repo, pr_number: u64) -> Result<PullRequest> {
        self.github_get(&format!("/repos/{}/{}/pulls/{pr_number}", repo.owner, repo.name))
    }

    fn list_statuses(&self, repo: &Repo, sha: &str) -> Result<Vec<CommitStatus>> {
        let path = format!("/repos/{}/{}/commits/{sha}/status", repo.owner, repo.name);
        let response: CombinedStatusResponse = self.github_get(&path)?;
        Ok(response.statuses)
    }

    fn recheck_cla(&self, repo: &Repo, pr_number: u64) -> Result<()> {
        let url = format!(
            "https://cla-assistant.io/check/{}/{}?pullRequest={pr_number}",
            repo.owner, repo.name
        );
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
    description: Option<String>,
    target_url: Option<String>,
}

#[derive(Serialize)]
struct ClaStatusOutput {
    sha: String,
    state: Option<String>,
    description: Option<String>,
    target_url: Option<String>,
}

impl ClaStatusOutput {
    fn missing(sha: String) -> Self {
        Self {
            sha,
            state: None,
            description: None,
            target_url: None,
        }
    }
}
