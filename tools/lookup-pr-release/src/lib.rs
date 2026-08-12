use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::process::Command;
use std::sync::OnceLock;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PullRequestRef {
    pub repo: String,
    pub number: u64,
}

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

pub fn release_section(body: &str) -> Result<&str> {
    let mut start = None;
    let mut level = 0;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let hashes = trimmed.bytes().take_while(|byte| *byte == b'#').count();
        let heading = hashes > 0 && trimmed.as_bytes().get(hashes) == Some(&b' ');
        if heading && trimmed[hashes + 1..].trim().eq_ignore_ascii_case("Must be released") {
            start = Some(offset + line.len());
            level = hashes;
        } else if start.is_some() && heading && hashes <= level {
            return Ok(&body[start.expect("checked")..offset]);
        }
        offset += line.len();
    }
    start
        .map(|start| &body[start..])
        .ok_or_else(|| anyhow!("PR description is missing the `Must be released` section"))
}

pub fn references(section: &str, current_repo: &str) -> Result<BTreeSet<PullRequestRef>> {
    static REFERENCE: OnceLock<Regex> = OnceLock::new();
    let regex = REFERENCE.get_or_init(|| {
        Regex::new(r"(?x)(?:https?://github\.com/)?(?:(?<owner>[A-Za-z0-9_.-]+)/)?(?:(?<repo>[A-Za-z0-9_.-]+))?\#|https?://github\.com/(?<url_owner>[A-Za-z0-9_.-]+)/(?<url_repo>[A-Za-z0-9_.-]+)/pull/").unwrap()
    });
    let (current_owner, _) = current_repo
        .split_once('/')
        .ok_or_else(|| anyhow!("repo must be in owner/name form"))?;
    let cleaned = strip_ignored_markdown(section);
    let mut result = BTreeSet::new();
    for captures in regex.captures_iter(&cleaned) {
        let after = &cleaned[captures.get(0).expect("whole match").end()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            continue;
        }
        let number = after[..digits].parse()?;
        let repo = if let (Some(owner), Some(repo)) = (captures.name("url_owner"), captures.name("url_repo")) {
            format!("{}/{}", owner.as_str(), repo.as_str())
        } else if let Some(repo) = captures.name("repo") {
            let owner = captures.name("owner").map_or(current_owner, |owner| owner.as_str());
            format!("{owner}/{}", repo.as_str())
        } else {
            current_repo.to_owned()
        };
        result.insert(PullRequestRef { repo, number });
    }
    Ok(result)
}

fn strip_ignored_markdown(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_comment = false;
    let mut in_fence = false;
    for line in input.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            output.push('\n');
            continue;
        }
        let mut remaining = line;
        while !in_fence && !remaining.is_empty() {
            if in_comment {
                if let Some(end) = remaining.find("-->") {
                    remaining = &remaining[end + 3..];
                    in_comment = false;
                } else {
                    break;
                }
            } else if let Some(start) = remaining.find("<!--") {
                output.push_str(&remaining[..start]);
                remaining = &remaining[start + 4..];
                in_comment = true;
            } else {
                output.push_str(remaining);
                break;
            }
        }
        output.push('\n');
    }
    output
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
    fn extracts_only_the_release_section() {
        let body = "# Intro\n# Must be released\n#12 repo#13 owner/repo#14\n## Detail\n#15\n# Testing\n#16\n";
        assert_eq!(
            release_section(body).unwrap(),
            "#12 repo#13 owner/repo#14\n## Detail\n#15\n"
        );
    }

    #[test]
    fn recognizes_reference_forms_and_ignores_comments_and_fences() {
        let section =
            "#12 SpacetimeDBPrivate#13 other/repo#14 https://github.com/x/y/pull/15\n<!-- #16 -->\n```\n#17\n```";
        let refs = references(section, "clockworklabs/SpacetimeDB").unwrap();
        assert!(refs.contains(&PullRequestRef {
            repo: "clockworklabs/SpacetimeDB".into(),
            number: 12
        }));
        assert!(refs.contains(&PullRequestRef {
            repo: "clockworklabs/SpacetimeDBPrivate".into(),
            number: 13
        }));
        assert!(refs.contains(&PullRequestRef {
            repo: "other/repo".into(),
            number: 14
        }));
        assert!(refs.contains(&PullRequestRef {
            repo: "x/y".into(),
            number: 15
        }));
        assert_eq!(refs.len(), 4);
    }

    #[test]
    fn matches_only_subject_suffix() {
        assert!(subject_matches("Change thing (#42)\n\nBody", "(#42)"));
        assert!(!subject_matches("Change thing (#42) extra", "(#42)"));
    }
}
