use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::process::Command;

const RELEASE_REPO: &str = "clockworklabs/SpacetimeDB";

// TODO: Consolidate this with other half-implementations of GitHub CLI calls across the codebase.
#[derive(Deserialize)]
pub struct PullRequest {
    pub created_at: String,
    pub body: Option<String>,
}

#[derive(Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
    pub published_at: Option<String>,
    pub draft: bool,
}

#[derive(Deserialize)]
pub struct CompareResponse {
    pub commits: Vec<CompareCommit>,
}

#[derive(Deserialize)]
pub struct CompareCommit {
    pub commit: CompareCommitData,
}

#[derive(Deserialize)]
pub struct CompareCommitData {
    pub message: String,
}

pub trait Github {
    fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T>;
    fn get_paginated<T: DeserializeOwned>(&self, endpoint: &str) -> Result<Vec<T>>;

    fn pull_request(&self, repo: &str, number: u64) -> Result<PullRequest> {
        self.get(&format!("repos/{repo}/pulls/{number}"))
    }

    fn releases(&self) -> Result<Vec<GithubRelease>> {
        let pages: Vec<Vec<GithubRelease>> =
            self.get_paginated(&format!("repos/{RELEASE_REPO}/releases?per_page=100"))?;
        Ok(pages.into_iter().flatten().collect())
    }

    fn release_by_tag(&self, tag: &str) -> Result<GithubRelease> {
        self.get(&format!("repos/{RELEASE_REPO}/releases/tags/{tag}"))
    }

    fn commit_range(&self, repo: &str, base: &str, head: &str) -> Result<Vec<CompareCommit>> {
        let pages: Vec<CompareResponse> =
            self.get_paginated(&format!("repos/{repo}/compare/{base}...{head}?per_page=100"))?;
        Ok(pages.into_iter().flat_map(|page| page.commits).collect())
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

    fn get_paginated<T: DeserializeOwned>(&self, endpoint: &str) -> Result<Vec<T>> {
        let output = Command::new("gh")
            .args(["api", "--paginate", "--slurp", endpoint])
            .output()
            .with_context(|| format!("failed to run `gh api --paginate --slurp {endpoint}`"))?;
        if !output.status.success() {
            bail!(
                "GitHub API request {endpoint} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        serde_json::from_slice(&output.stdout).with_context(|| format!("invalid paginated response from {endpoint}"))
    }
}
