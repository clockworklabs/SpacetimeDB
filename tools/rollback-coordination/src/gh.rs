//! Thin wrappers around GitHub API requests. Business logic belongs in the
//! calling coordination modules, not here.

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

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
        let output = Command::new("gh")
            .args(["api", endpoint])
            .output()
            .with_context(|| format!("failed to run `gh api {endpoint}`"))?;
        if !output.status.success() {
            bail!(
                "GitHub API request {endpoint} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        serde_json::from_slice(&output.stdout).with_context(|| format!("invalid response from {endpoint}"))
    }

    fn repository_info(&self, path: &Path) -> Result<RepositoryInfo> {
        let output = Command::new("gh")
            .args(["repo", "view", "--json", "nameWithOwner"])
            .current_dir(path)
            .output()
            .with_context(|| format!("failed to run `gh repo view` in {}", path.display()))?;
        if !output.status.success() {
            bail!(
                "`gh repo view` failed in {}: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        serde_json::from_slice(&output.stdout).context("invalid response from `gh repo view`")
    }
}
