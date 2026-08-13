use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, PartialEq)]
pub struct Release {
    pub tag: String,
    pub created_at: String,
}

pub trait Github {
    fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T>;
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
}

#[derive(Deserialize)]
struct PullRequest {
    created_at: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    published_at: Option<String>,
    draft: bool,
}

#[derive(Deserialize)]
struct CompareResponse {
    commits: Vec<CompareCommit>,
}

#[derive(Deserialize)]
struct CompareCommit {
    commit: CompareCommitData,
}

#[derive(Deserialize)]
struct CompareCommitData {
    message: String,
}

#[derive(Debug)]
struct PublishedRelease {
    tag: String,
    published_at: String,
}

pub fn lookup(client: &impl Github, repo: &str, pr_number: u64, verbose: bool) -> Result<Option<Release>> {
    trace(verbose, format_args!("Looking up {repo}#{pr_number}"));
    let pr: PullRequest = client.get(&format!("repos/{repo}/pulls/{pr_number}"))?;
    trace(verbose, format_args!("PR was created at {}", pr.created_at));
    let releases = load_releases(client, &pr.created_at, verbose)?;
    trace(verbose, format_args!("Selected {} releases", releases.len()));

    let suffix = format!("(#{pr_number})");
    for index in 0..releases.len() {
        if releases[index].published_at < pr.created_at {
            continue;
        }
        let base = index.checked_sub(1).map(|previous| releases[previous].tag.as_str());
        match base {
            Some(base) => trace(
                verbose,
                format_args!(
                    "Checking commits {base}...{} for release {}",
                    releases[index].tag, releases[index].tag
                ),
            ),
            None => trace(
                verbose,
                format_args!(
                    "Checking history through {} for first tag {}",
                    releases[index].tag, releases[index].tag
                ),
            ),
        }
        if range_contains_pr(client, repo, base, &releases[index].tag, &suffix)? {
            trace(verbose, format_args!("Matched {}", releases[index].tag));
            return Ok(Some(Release {
                tag: releases[index].tag.clone(),
                created_at: releases[index].published_at.clone(),
            }));
        }
    }
    trace(verbose, format_args!("No release tag contains {repo}#{pr_number}"));
    Ok(None)
}

#[allow(clippy::disallowed_macros)]
fn trace(verbose: bool, message: std::fmt::Arguments<'_>) {
    if verbose {
        eprintln!("{message}");
    }
}

fn load_releases(client: &impl Github, pr_created_at: &str, verbose: bool) -> Result<Vec<PublishedRelease>> {
    const RELEASE_REPO: &str = "clockworklabs/SpacetimeDB";
    let mut result = Vec::new();
    for page in 1.. {
        trace(verbose, format_args!("Loading public release page {page}"));
        let releases: Vec<GithubRelease> =
            client.get(&format!("repos/{RELEASE_REPO}/releases?per_page=100&page={page}"))?;
        trace(
            verbose,
            format_args!("Public release page {page} returned {} entries", releases.len()),
        );
        if releases.is_empty() {
            break;
        }
        let page_len = releases.len();
        let mut found_preceding = false;
        for release in releases {
            if release.draft {
                trace(verbose, format_args!("Ignoring draft release {}", release.tag_name));
                continue;
            }
            let Some(published_at) = release.published_at else {
                trace(
                    verbose,
                    format_args!("Ignoring unpublished release {}", release.tag_name),
                );
                continue;
            };
            let precedes_pr = published_at.as_str() < pr_created_at;
            trace(
                verbose,
                format_args!("Selected release {} published at {published_at}", release.tag_name),
            );
            result.push(PublishedRelease {
                tag: release.tag_name,
                published_at,
            });
            if precedes_pr {
                found_preceding = true;
                break;
            }
        }
        if found_preceding || page_len < 100 {
            break;
        }
    }
    result.reverse();
    Ok(result)
}

fn range_contains_pr(client: &impl Github, repo: &str, base: Option<&str>, head: &str, suffix: &str) -> Result<bool> {
    for page in 1.. {
        let commits = if let Some(base) = base {
            let response: CompareResponse = client.get(&format!(
                "repos/{repo}/compare/{base}...{head}?per_page=100&page={page}"
            ))?;
            response.commits
        } else {
            client.get::<Vec<CompareCommit>>(&format!("repos/{repo}/commits?sha={head}&per_page=100&page={page}"))?
        };
        let page_len = commits.len();
        // N.B. that this is based on commit subject lines, so it can technically be spoofed.
        if commits
            .iter()
            .any(|commit| subject_matches(&commit.commit.message, suffix))
        {
            return Ok(true);
        }
        if page_len < 100 {
            return Ok(false);
        }
    }
    unreachable!()
}

fn subject_matches(message: &str, suffix: &str) -> bool {
    message.lines().next().is_some_and(|subject| subject.ends_with(suffix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::collections::HashMap;

    struct FakeGithub(HashMap<String, Value>);

    impl Github for FakeGithub {
        fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
            serde_json::from_value(
                self.0
                    .get(endpoint)
                    .unwrap_or_else(|| panic!("unexpected endpoint {endpoint}"))
                    .clone(),
            )
            .map_err(Into::into)
        }
    }

    #[test]
    fn finds_the_earliest_tag_using_adjacent_ranges() {
        let github = FakeGithub(HashMap::from([
            (
                "repos/o/r/pulls/42".into(),
                json!({ "created_at": "2024-01-02T00:00:00Z" }),
            ),
            (
                "repos/clockworklabs/SpacetimeDB/releases?per_page=100&page=1".into(),
                json!([
                    { "tag_name": "v2", "published_at": "2024-01-03T00:00:00Z", "draft": false },
                    { "tag_name": "ignored", "published_at": null, "draft": true },
                    { "tag_name": "v1", "published_at": "2024-01-01T00:00:00Z", "draft": false }
                ]),
            ),
            (
                "repos/o/r/compare/v1...v2?per_page=100&page=1".into(),
                json!({ "commits": [{ "commit": { "message": "The change (#42)\n\nDetails" } }] }),
            ),
        ]));

        assert_eq!(
            lookup(&github, "o/r", 42, false).unwrap(),
            Some(Release {
                tag: "v2".into(),
                created_at: "2024-01-03T00:00:00Z".into()
            })
        );
    }

    fn assert_live_release(pr_number: u64, expected_tag: &str) {
        let release = lookup(&Gh, "clockworklabs/SpacetimeDB", pr_number, false)
            .unwrap_or_else(|error| panic!("failed to look up SpacetimeDB#{pr_number}: {error:#}"))
            .unwrap_or_else(|| panic!("SpacetimeDB#{pr_number} has not been released"));
        assert_eq!(
            release.tag, expected_tag,
            "unexpected earliest release for SpacetimeDB#{pr_number}"
        );
    }

    #[test]
    fn live_pr_5255_was_released_in_v2_6_0() {
        assert_live_release(5255, "v2.6.0");
    }

    #[test]
    fn live_pr_5645_was_released_in_v2_8_0() {
        assert_live_release(5645, "v2.8.0");
    }

    #[test]
    fn loads_release_pages_until_it_finds_the_preceding_release() {
        let mut first_page = Vec::new();
        for number in 0..100 {
            first_page.push(json!({
                "tag_name": format!("v-new-{number}"),
                "published_at": "2024-01-03T00:00:00Z",
                "draft": false
            }));
        }
        let github = FakeGithub(HashMap::from([
            (
                "repos/clockworklabs/SpacetimeDB/releases?per_page=100&page=1".into(),
                Value::Array(first_page),
            ),
            (
                "repos/clockworklabs/SpacetimeDB/releases?per_page=100&page=2".into(),
                json!([
                    { "tag_name": "draft", "published_at": "2024-01-01T00:00:00Z", "draft": true },
                    { "tag_name": "v-base", "published_at": "2024-01-01T00:00:00Z", "draft": false },
                    { "tag_name": "too-old", "published_at": "2023-01-01T00:00:00Z", "draft": false }
                ]),
            ),
        ]));

        let releases = load_releases(&github, "2024-01-02T00:00:00Z", false).unwrap();
        assert_eq!(releases.first().unwrap().tag, "v-base");
        assert_eq!(releases.len(), 101);
    }

    #[test]
    fn matches_only_subject_suffix() {
        assert!(subject_matches("Change thing (#42)\n\nBody", "(#42)"));
        assert!(!subject_matches("Change thing (#42) extra", "(#42)"));
    }
}
