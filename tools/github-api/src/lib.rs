//! Small synchronous GitHub REST API client backed by the authenticated `gh` CLI.

mod related_prs;

use anyhow::{bail, ensure, Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::Write;
use std::process::{Command, Stdio};

pub use related_prs::{prs_mentioned, prs_mentioning, RelatedPullRequest};

/// GitHub REST API version sent with every request.
pub const API_VERSION: &str = "2022-11-28";

/// Synchronous GitHub API access backed by the authenticated `gh` CLI.
#[derive(Clone, Debug, Default)]
pub struct GithubApi {
    token: Option<String>,
}

impl GithubApi {
    /// Creates a client that passes `token` to every `gh` invocation.
    pub fn new(token: impl Into<String>) -> Result<Self> {
        let token = token.into();
        ensure!(!token.trim().is_empty(), "GitHub API token must not be empty");
        Ok(Self { token: Some(token) })
    }

    /// Creates a client that uses `GH_TOKEN` when present and otherwise lets
    /// `gh` use credentials configured by `gh auth login`.
    pub fn from_environment() -> Self {
        Self {
            token: env::var("GH_TOKEN").ok().filter(|token| !token.trim().is_empty()),
        }
    }

    fn command(&self, args: &[String], stdin: Option<&[u8]>) -> Result<String> {
        let mut command = Command::new("gh");
        command.args(args);
        if let Some(token) = &self.token {
            command.env("GH_TOKEN", token);
        }
        if stdin.is_some() {
            command.stdin(Stdio::piped());
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .with_context(|| format!("failed to run `gh {}`", args.join(" ")))?;
        if let Some(stdin) = stdin {
            child
                .stdin
                .take()
                .context("failed to open gh stdin")?
                .write_all(stdin)
                .context("failed to write gh stdin")?;
        }
        let output = child.wait_with_output().context("failed to wait for gh")?;
        if !output.status.success() {
            bail!(
                "`gh {}` failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim_end()
            );
        }
        String::from_utf8(output.stdout).context("gh stdout was not valid UTF-8")
    }

    fn api_get_output(&self, path: &str, query: &[(&str, String)], paginate: bool, accept: &str) -> Result<String> {
        let mut args = vec!["api".to_owned(), "--method".to_owned(), "GET".to_owned()];
        if paginate {
            args.extend(["--paginate".to_owned(), "--slurp".to_owned()]);
        }
        args.extend([
            "-H".to_owned(),
            format!("Accept: {accept}"),
            "-H".to_owned(),
            format!("X-GitHub-Api-Version: {API_VERSION}"),
        ]);
        for (name, value) in query {
            args.extend(["--raw-field".to_owned(), format!("{name}={value}")]);
        }
        args.push(path.to_owned());
        self.command(&args, None)
    }

    /// Fetches and deserializes one GitHub REST API response.
    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.get_with_query(path, &[])
    }

    /// Fetches and deserializes one GitHub REST API response with encoded query fields.
    pub fn get_with_query<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        let output = self.api_get_output(path, query, false, "application/vnd.github+json")?;
        serde_json::from_str(&output).with_context(|| format!("failed to parse GitHub response for {path}"))
    }

    /// Fetches every page and preserves GitHub's page boundaries.
    pub fn get_paginated<P: DeserializeOwned>(&self, path: &str) -> Result<Vec<P>> {
        self.get_paginated_with_query(path, &[])
    }

    /// Fetches every page with encoded query fields and preserves page boundaries.
    pub fn get_paginated_with_query<P: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Vec<P>> {
        let output = self.api_get_output(path, query, true, "application/vnd.github+json")?;
        serde_json::from_str(&output).with_context(|| format!("failed to parse paginated GitHub response for {path}"))
    }

    /// Reads a repository file as raw text at the selected Git ref.
    pub fn repository_file(&self, repo: &str, path: &str, git_ref: &str) -> Result<String> {
        self.api_get_output(
            &format!("/repos/{repo}/contents/{path}"),
            &[("ref", git_ref.to_owned())],
            false,
            "application/vnd.github.raw+json",
        )
    }

    /// Fetches a pull request using the authenticated `gh pr view --json` projection.
    pub fn pull_request_view<T: DeserializeOwned>(&self, repo: &str, number: u64, fields: &str) -> Result<T> {
        let args = [
            "pr".to_owned(),
            "view".to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            repo.to_owned(),
            "--json".to_owned(),
            fields.to_owned(),
        ];
        let output = self.command(&args, None)?;
        serde_json::from_str(&output).with_context(|| format!("failed to parse pull request view for {repo}#{number}"))
    }

    /// Sends a JSON POST request and deserializes the response.
    pub fn post<I: Serialize, O: DeserializeOwned>(&self, path: &str, input: &I) -> Result<O> {
        let input = serde_json::to_vec(input).context("failed to serialize GitHub request")?;
        let args = [
            "api".to_owned(),
            "--method".to_owned(),
            "POST".to_owned(),
            "--input".to_owned(),
            "-".to_owned(),
            "-H".to_owned(),
            "Accept: application/vnd.github+json".to_owned(),
            "-H".to_owned(),
            format!("X-GitHub-Api-Version: {API_VERSION}"),
            path.to_owned(),
        ];
        let output = self.command(&args, Some(&input))?;
        serde_json::from_str(&output).with_context(|| format!("failed to parse GitHub response for {path}"))
    }

    /// Sends a POST request whose response body is not needed.
    pub fn post_empty(&self, path: &str) -> Result<()> {
        let args = [
            "api".to_owned(),
            "--method".to_owned(),
            "POST".to_owned(),
            "-H".to_owned(),
            "Accept: application/vnd.github+json".to_owned(),
            "-H".to_owned(),
            format!("X-GitHub-Api-Version: {API_VERSION}"),
            path.to_owned(),
        ];
        self.command(&args, None)?;
        Ok(())
    }

    pub fn pull_request(&self, repo: &str, number: u64) -> Result<PullRequest> {
        self.get(&format!("/repos/{repo}/pulls/{number}"))
    }

    pub fn branch(&self, repo: &str, name: &str) -> Result<Branch> {
        self.get(&format!("/repos/{repo}/branches/{name}"))
    }

    pub fn git_tree(&self, repo: &str, sha: &str) -> Result<GitTree> {
        self.get(&format!("/repos/{repo}/git/trees/{sha}"))
    }

    /// Fetches all workflow runs matching server-side filters.
    pub fn workflow_runs_for_repo(&self, repo: &str, filters: &[(&str, String)]) -> Result<Vec<WorkflowRun>> {
        let mut query = filters.to_vec();
        query.push(("per_page", "100".to_owned()));
        let pages: Vec<WorkflowRunsPage> =
            self.get_paginated_with_query(&format!("/repos/{repo}/actions/runs"), &query)?;
        Ok(pages.into_iter().flat_map(|page| page.workflow_runs).collect())
    }

    pub fn workflow_jobs(&self, repo: &str, run_id: u64) -> Result<Vec<WorkflowJob>> {
        let pages: Vec<WorkflowJobsPage> = self.get_paginated_with_query(
            &format!("/repos/{repo}/actions/runs/{run_id}/jobs"),
            &[("filter", "latest".to_owned()), ("per_page", "100".to_owned())],
        )?;
        Ok(pages.into_iter().flat_map(|page| page.jobs).collect())
    }

    /// Fetches at most the latest 100 workflow runs matching server-side filters.
    pub fn latest_workflow_runs(
        &self,
        repo: &str,
        workflow: &str,
        filters: &[(&str, String)],
    ) -> Result<Vec<WorkflowRun>> {
        let mut query = filters.to_vec();
        query.push(("per_page", "100".to_owned()));
        let page: WorkflowRunsPage =
            self.get_with_query(&format!("/repos/{repo}/actions/workflows/{workflow}/runs"), &query)?;
        Ok(page.workflow_runs)
    }

    pub fn workflow_run(&self, repo: &str, run_id: u64) -> Result<WorkflowRunStatus> {
        self.get(&format!("/repos/{repo}/actions/runs/{run_id}"))
    }

    pub fn rerun_failed_jobs(&self, repo: &str, run_id: u64) -> Result<()> {
        self.post_empty(&format!("/repos/{repo}/actions/runs/{run_id}/rerun-failed-jobs"))
    }

    pub fn rerun_workflow(&self, repo: &str, run_id: u64) -> Result<()> {
        self.post_empty(&format!("/repos/{repo}/actions/runs/{run_id}/rerun"))
    }

    pub fn cancel_workflow_run(&self, repo: &str, run_id: u64) -> Result<()> {
        self.post_empty(&format!("/repos/{repo}/actions/runs/{run_id}/cancel"))
    }

    pub fn dispatch_workflow<T: Serialize>(
        &self,
        repo: &str,
        workflow: &str,
        git_ref: &str,
        inputs: T,
    ) -> Result<DispatchResponse> {
        self.post(
            &format!("/repos/{repo}/actions/workflows/{workflow}/dispatches"),
            &DispatchWorkflow {
                ref_name: git_ref,
                inputs,
                return_run_details: true,
            },
        )
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub state: String,
    pub head: PullRequestRef,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PullRequestRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub sha: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Branch {
    pub commit: Commit,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct GitTree {
    pub tree: Vec<GitTreeEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct GitTreeEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub sha: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct WorkflowRunsPage {
    pub workflow_runs: Vec<WorkflowRun>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct WorkflowRun {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub display_title: String,
    pub event: String,
    pub head_sha: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub run_attempt: u64,
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub pull_requests: Vec<WorkflowRunPullRequest>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct WorkflowRunPullRequest {
    pub number: u64,
    pub head: WorkflowRunRef,
    pub base: WorkflowRunRef,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct WorkflowRunRef {
    pub sha: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct WorkflowJobsPage {
    pub jobs: Vec<WorkflowJob>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct WorkflowJob {
    pub name: String,
    pub status: Option<String>,
    pub conclusion: Option<String>,
    pub html_url: Option<String>,
    pub labels: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct WorkflowRunStatus {
    pub id: u64,
    pub status: String,
    pub conclusion: Option<String>,
    pub run_attempt: u64,
    pub html_url: String,
}

#[derive(Debug, Serialize)]
pub struct DispatchWorkflow<'a, T> {
    #[serde(rename = "ref")]
    pub ref_name: &'a str,
    pub inputs: T,
    pub return_run_details: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct DispatchResponse {
    pub workflow_run_id: u64,
    pub html_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_request_serializes_the_workflow_ref_for_github() {
        let request = DispatchWorkflow {
            ref_name: "master",
            inputs: serde_json::json!({"key": "value"}),
            return_run_details: true,
        };
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "ref": "master",
                "inputs": {"key": "value"},
                "return_run_details": true
            })
        );
    }
}
