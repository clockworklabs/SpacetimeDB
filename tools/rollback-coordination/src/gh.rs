//! Thin wrappers around GitHub API requests. Business logic belongs in the
//! calling coordination modules, not here.

use anyhow::{Context, Result};
use duct::cmd;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::path::Path;

// TODO: Consolidate this with other half-implementations of GitHub CLI calls across the codebase.
#[derive(Deserialize)]
pub struct PullRequest {
    pub body: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryInfo {
    pub name_with_owner: String,
}

pub trait Github {
    fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T>;
    fn repository_info(&self, path: &Path) -> Result<RepositoryInfo>;

    fn pull_request(&self, repo: &str, number: u64) -> Result<PullRequest> {
        self.get(&format!("repos/{repo}/pulls/{number}"))
    }
}

pub struct Gh;

impl Github for Gh {
    fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
        let output = cmd!("gh", "api", endpoint)
            .read()
            .with_context(|| format!("failed to run `gh api {endpoint}`"))?;
        serde_json::from_str(&output).with_context(|| format!("invalid response from {endpoint}"))
    }

    fn repository_info(&self, path: &Path) -> Result<RepositoryInfo> {
        let output = cmd!("gh", "repo", "view", "--json", "nameWithOwner")
            .dir(path)
            .read()
            .with_context(|| format!("failed to run `gh repo view` in {}", path.display()))?;
        serde_json::from_str(&output).context("invalid response from `gh repo view`")
    }
}
