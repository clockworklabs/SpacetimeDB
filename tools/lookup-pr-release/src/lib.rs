use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use std::collections::BTreeSet;
use std::sync::OnceLock;

pub mod gh;

pub use gh::{Gh, Github};

const ROLLBACK_SAFETY_HEADING: &str = "Rollback safety impact";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Release {
    pub tag: String,
    pub published_at: String,
}

impl Ord for Release {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.published_at
            .cmp(&other.published_at)
            .then_with(|| self.tag.cmp(&other.tag))
    }
}

impl PartialOrd for Release {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub fn lookup_release_for(client: &impl Github, repo: &str, pr_number: u64) -> Result<Option<Release>> {
    tracing::info!("Looking up {repo}#{pr_number}");
    let pr = client.pull_request(repo, pr_number)?;
    tracing::info!("PR was created at {}", pr.created_at);
    let releases = load_releases(client)?;
    tracing::info!("Selected {} releases", releases.len());

    let suffix = format!("(#{pr_number})");
    let first_candidate = releases
        .partition_point(|release| release.published_at < pr.created_at)
        .checked_sub(1)
        .with_context(|| format!("no published release predates {repo}#{pr_number}"))?;
    for pair in releases[first_candidate..].windows(2) {
        let [base, release] = pair else { unreachable!() };
        tracing::info!(
            "Checking commits {}...{} for release {}",
            base.tag,
            release.tag,
            release.tag
        );

        // N.B. that this is based on commit subject lines, so it can technically be spoofed.
        if client
            .commit_range(repo, &base.tag, &release.tag)?
            .iter()
            .any(|commit| subject_matches(&commit.commit.message, &suffix))
        {
            tracing::info!("Matched {}", release.tag);
            return Ok(Some(release.clone()));
        }
    }
    tracing::info!("No release tag contains {repo}#{pr_number}");
    Ok(None)
}

fn load_releases(client: &impl Github) -> Result<Vec<Release>> {
    let mut result = Vec::new();
    for release in client.releases()? {
        if release.draft {
            tracing::info!("Ignoring draft release {}", release.tag_name);
            continue;
        }
        result.push(Release {
            tag: release.tag_name,
            published_at: release
                .published_at
                .context("Non-draft release expected to have published_at")?,
        });
    }
    result.sort();
    Ok(result)
}

fn subject_matches(message: &str, suffix: &str) -> bool {
    message.lines().next().is_some_and(|subject| subject.ends_with(suffix))
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PullRequestRef {
    repo: String,
    number: u64,
}

/// Computes the earliest release to which all requested pull requests can safely be rolled back.
pub fn earliest_rollback_point(
    github: &impl Github,
    repo: &str,
    pr_numbers: &[u64],
    allowed_repos: &BTreeSet<String>,
    strict_template: Option<&str>,
) -> Result<Option<Release>> {
    let results = pr_numbers
        .iter()
        .map(|&number| earliest_rollback_point_for_pr(github, repo, allowed_repos, strict_template, number))
        .collect::<Vec<_>>();
    Ok(results.collect_all()?.into_iter().flatten().max())
}

fn earliest_rollback_point_for_pr(
    github: &impl Github,
    repo: &str,
    allowed_repos: &BTreeSet<String>,
    strict_template: Option<&str>,
    number: u64,
) -> Result<Option<Release>> {
    let pr = github
        .pull_request(repo, number)
        .with_context(|| format!("failed to load {repo}#{number}"))?;
    let body = pr.body.as_deref().unwrap_or_default();
    let Some(dependencies) = rollback_dependencies(body, repo, strict_template)
        .with_context(|| format!("failed to validate {repo}#{number}"))?
    else {
        tracing::info!("{repo}#{number} has no `{ROLLBACK_SAFETY_HEADING}` section");
        return Ok(None);
    };
    tracing::info!(
        "{repo}#{number} has {} rollback prerequisite{}",
        dependencies.len(),
        if dependencies.len() == 1 { "" } else { "s" }
    );

    let results = dependencies
        .into_iter()
        .map(|dependency| {
            if !allowed_repos.contains(&dependency.repo) {
                bail!(
                    "{}#{} is not in an allowed repository (required by {repo}#{number})",
                    dependency.repo,
                    dependency.number
                );
            }
            let release = lookup_release_for(github, &dependency.repo, dependency.number)
                .with_context(|| {
                    format!(
                        "release lookup failed for {}#{} (required by {repo}#{number})",
                        dependency.repo, dependency.number
                    )
                })?
                .with_context(|| {
                    format!(
                        "{}#{} has not been released (required by {repo}#{number})",
                        dependency.repo, dependency.number
                    )
                })?;
            tracing::info!(
                "{}#{} was released in {} (required by {repo}#{number})",
                dependency.repo,
                dependency.number,
                release.tag
            );
            Ok(Some(release))
        })
        .collect::<Vec<_>>();
    Ok(results.collect_all()?.into_iter().flatten().max())
}

pub fn rollback_point_from_str(github: &impl Github, tag: &str) -> Result<Release> {
    let release = github
        .release_by_tag(tag)
        .with_context(|| format!("failed to find published release {tag}"))?;
    if release.draft {
        bail!("release {tag} is still a draft");
    }
    let published_at = release
        .published_at
        .with_context(|| format!("release {tag} has no publication time"))?;
    Ok(Release {
        tag: release.tag_name,
        published_at,
    })
}

trait CollectAll<T> {
    fn collect_all(self) -> Result<Vec<T>>;
}

impl<T> CollectAll<T> for Vec<Result<T>> {
    fn collect_all(self) -> Result<Vec<T>> {
        let mut values = Vec::new();
        let mut errors = Vec::new();

        for result in self {
            match result {
                Ok(value) => values.push(value),
                Err(err) => errors.push(err),
            }
        }

        if errors.is_empty() {
            Ok(values)
        } else {
            Err(anyhow!(
                "{}",
                errors
                    .into_iter()
                    .map(|error| format!("{error:#}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ))
        }
    }
}

fn rollback_safety_section(body: &str) -> Option<&str> {
    let mut start = None;
    let mut level = 0;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let hashes = trimmed.bytes().take_while(|byte| *byte == b'#').count();
        let heading = hashes > 0 && trimmed.as_bytes().get(hashes) == Some(&b' ');
        if heading
            && trimmed[hashes + 1..]
                .trim()
                .eq_ignore_ascii_case(ROLLBACK_SAFETY_HEADING)
        {
            start = Some(offset + line.len());
            level = hashes;
        } else if start.is_some() && heading && hashes <= level {
            return Some(&body[start.expect("checked")..offset]);
        }
        offset += line.len();
    }
    start.map(|start| &body[start..])
}

fn normalized_section(section: &str) -> String {
    section
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

fn rollback_dependencies(
    body: &str,
    current_repo: &str,
    strict_template: Option<&str>,
) -> Result<Option<BTreeSet<PullRequestRef>>> {
    let Some(section) = rollback_safety_section(body) else {
        if strict_template.is_some() {
            bail!("PR description is missing the `{ROLLBACK_SAFETY_HEADING}` section");
        }
        return Ok(None);
    };
    let dependencies = references(section, current_repo)?;

    if let Some(template) = strict_template {
        let template_section = rollback_safety_section(template)
            .ok_or_else(|| anyhow!("pull request template is missing the `{ROLLBACK_SAFETY_HEADING}` section"))?;
        if normalized_section(section) == normalized_section(template_section) {
            bail!(
                "the `{ROLLBACK_SAFETY_HEADING}` section is unchanged from the pull request template; add `n/a` or list prerequisite PRs"
            );
        }
        if dependencies.is_empty() && !contains_na(section) {
            bail!("the `{ROLLBACK_SAFETY_HEADING}` section must contain `n/a` or at least one prerequisite PR");
        }
    }

    Ok(Some(dependencies))
}

fn contains_na(section: &str) -> bool {
    static NA: OnceLock<Regex> = OnceLock::new();
    NA.get_or_init(|| Regex::new(r"(?i)(?:^|[^A-Za-z0-9_])n/a(?:$|[^A-Za-z0-9_])").unwrap())
        .is_match(&strip_ignored_markdown(section))
}

fn references(section: &str, current_repo: &str) -> Result<BTreeSet<PullRequestRef>> {
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
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};
    use std::collections::HashMap;

    struct FakeGithub(HashMap<String, Value>);

    impl FakeGithub {
        fn response<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
            serde_json::from_value(
                self.0
                    .get(endpoint)
                    .unwrap_or_else(|| panic!("unexpected endpoint {endpoint}"))
                    .clone(),
            )
            .map_err(Into::into)
        }
    }

    impl Github for FakeGithub {
        fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
            self.response(endpoint)
        }

        fn get_paginated<T: DeserializeOwned>(&self, endpoint: &str) -> Result<Vec<T>> {
            self.response(endpoint)
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
                "repos/clockworklabs/SpacetimeDB/releases?per_page=100".into(),
                json!([[
                    { "tag_name": "v2", "published_at": "2024-01-03T00:00:00Z", "draft": false },
                    { "tag_name": "ignored", "published_at": null, "draft": true },
                    { "tag_name": "v1", "published_at": "2024-01-01T00:00:00Z", "draft": false }
                ]]),
            ),
            (
                "repos/o/r/compare/v1...v2?per_page=100".into(),
                json!([{ "commits": [{ "commit": { "message": "The change (#42)\n\nDetails" } }] }]),
            ),
        ]));

        assert_eq!(
            lookup_release_for(&github, "o/r", 42).unwrap(),
            Some(Release {
                tag: "v2".into(),
                published_at: "2024-01-03T00:00:00Z".into()
            })
        );
    }

    #[test]
    fn extracts_the_rollback_section() {
        let body = "# Intro\n# Rollback safety impact\n#12\n## Detail\n#13\n# Testing\n#14\n";
        assert_eq!(rollback_safety_section(body), Some("#12\n## Detail\n#13\n"));
    }

    #[test]
    fn parses_and_validates_rollback_dependencies_once() {
        const REPO: &str = "clockworklabs/SpacetimeDB";
        let template = "# Rollback safety impact\n\n<!-- instructions -->\n";

        assert_eq!(rollback_dependencies("", REPO, None).unwrap(), None);
        assert!(
            rollback_dependencies("# Rollback safety impact\nsafe to deploy", REPO, None)
                .unwrap()
                .unwrap()
                .is_empty()
        );
        assert!(
            rollback_dependencies("# Rollback safety impact\nN/A", REPO, Some(template))
                .unwrap()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            rollback_dependencies("# Rollback safety impact\n#12", REPO, Some(template)).unwrap(),
            Some(BTreeSet::from([PullRequestRef {
                repo: REPO.into(),
                number: 12,
            }]))
        );
        assert!(rollback_dependencies("# Rollback safety impact\nsafe to deploy", REPO, Some(template)).is_err());

        let unchanged = "# Rollback safety impact\r\n\r\n<!-- instructions -->   \r\n";
        assert!(rollback_dependencies(unchanged, REPO, Some(template)).is_err());
    }

    #[test]
    fn recognizes_references_and_ignores_comments_and_fences() {
        let section =
            "#12 SpacetimeDBPrivate#13 other/repo#14 https://github.com/x/y/pull/15\n<!-- #16 -->\n```\n#17\n```";
        let refs = references(section, "clockworklabs/SpacetimeDB").unwrap();
        assert_eq!(
            refs,
            BTreeSet::from([
                PullRequestRef {
                    repo: "clockworklabs/SpacetimeDB".into(),
                    number: 12,
                },
                PullRequestRef {
                    repo: "clockworklabs/SpacetimeDBPrivate".into(),
                    number: 13,
                },
                PullRequestRef {
                    repo: "other/repo".into(),
                    number: 14,
                },
                PullRequestRef {
                    repo: "x/y".into(),
                    number: 15,
                },
            ])
        );
    }

    #[test]
    fn rejects_dependencies_outside_the_allowlist() {
        let github = FakeGithub(HashMap::from([(
            "repos/o/r/pulls/1".into(),
            json!({
                "created_at": "2024-01-01T00:00:00Z",
                "body": "# Rollback safety impact\nother/repo#2"
            }),
        )]));
        let error = earliest_rollback_point(&github, "o/r", &[1], &BTreeSet::from(["o/r".into()]), None).unwrap_err();
        assert!(error
            .to_string()
            .contains("other/repo#2 is not in an allowed repository"));
    }

    #[test]
    fn parses_and_maxes_rollback_points_by_publication_order() {
        let github = FakeGithub(HashMap::from([
            (
                "repos/clockworklabs/SpacetimeDB/releases/tags/older".into(),
                json!({ "tag_name": "older", "published_at": "2024-01-01T00:00:00Z", "draft": false }),
            ),
            (
                "repos/clockworklabs/SpacetimeDB/releases/tags/hotfix".into(),
                json!({ "tag_name": "hotfix", "published_at": "2024-01-02T00:00:00Z", "draft": false }),
            ),
        ]));
        let older = rollback_point_from_str(&github, "older").unwrap();
        let hotfix = rollback_point_from_str(&github, "hotfix").unwrap();
        let point = older.max(hotfix);
        assert_eq!(point.tag, "hotfix");
    }

    #[test]
    fn no_prs_produce_no_rollback_point() {
        let point = earliest_rollback_point(
            &FakeGithub(HashMap::new()),
            "o/r",
            &[],
            &BTreeSet::from(["o/r".into()]),
            None,
        )
        .unwrap();
        assert_eq!(point, None);
    }

    #[test]
    fn strict_validation_is_an_individual_parameter() {
        let github = FakeGithub(HashMap::from([(
            "repos/o/r/pulls/1".into(),
            json!({ "created_at": "2024-01-01T00:00:00Z", "body": "" }),
        )]));
        let error = earliest_rollback_point(
            &github,
            "o/r",
            &[1],
            &BTreeSet::from(["o/r".into()]),
            Some("# Rollback safety impact\n"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("missing"));
    }

    #[test]
    fn combines_all_rollback_check_errors() {
        let results: Vec<Result<Option<Release>>> = vec![Err(anyhow!("first")), Ok(None), Err(anyhow!("second"))];
        let error = results.collect_all().unwrap_err();
        assert_eq!(error.to_string(), "first\nsecond");
    }

    fn assert_live_release(pr_number: u64, expected_tag: &str) {
        let release = lookup_release_for(&Gh, "clockworklabs/SpacetimeDB", pr_number)
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
    fn loads_all_release_pages() {
        let mut first_page = Vec::new();
        for number in 0..100 {
            first_page.push(json!({
                "tag_name": format!("v-new-{number}"),
                "published_at": "2024-01-03T00:00:00Z",
                "draft": false
            }));
        }
        let github = FakeGithub(HashMap::from([(
            "repos/clockworklabs/SpacetimeDB/releases?per_page=100".into(),
            json!([
                first_page,
                [
                    { "tag_name": "draft", "published_at": "2024-01-01T00:00:00Z", "draft": true },
                    { "tag_name": "v-base", "published_at": "2024-01-01T00:00:00Z", "draft": false },
                    { "tag_name": "too-old", "published_at": "2023-01-01T00:00:00Z", "draft": false }
                ]
            ]),
        )]));

        let releases = load_releases(&github).unwrap();
        assert_eq!(releases.first().unwrap().tag, "too-old");
        assert_eq!(releases.len(), 102);
    }

    #[test]
    fn finds_a_commit_on_a_later_compare_page() {
        let github = FakeGithub(HashMap::from([
            (
                "repos/o/r/pulls/42".into(),
                json!({ "created_at": "2024-01-02T00:00:00Z" }),
            ),
            (
                "repos/clockworklabs/SpacetimeDB/releases?per_page=100".into(),
                json!([[
                    { "tag_name": "v2", "published_at": "2024-01-03T00:00:00Z", "draft": false },
                    { "tag_name": "v1", "published_at": "2024-01-01T00:00:00Z", "draft": false }
                ]]),
            ),
            (
                "repos/o/r/compare/v1...v2?per_page=100".into(),
                json!([
                    { "commits": [{ "commit": { "message": "Another change (#1)" } }] },
                    { "commits": [{ "commit": { "message": "The change (#42)" } }] }
                ]),
            ),
        ]));

        assert_eq!(lookup_release_for(&github, "o/r", 42).unwrap().unwrap().tag, "v2");
    }

    #[test]
    fn advances_the_base_for_each_candidate_release() {
        let github = FakeGithub(HashMap::from([
            (
                "repos/o/r/pulls/42".into(),
                json!({ "created_at": "2024-01-02T00:00:00Z" }),
            ),
            (
                "repos/clockworklabs/SpacetimeDB/releases?per_page=100".into(),
                json!([[
                    { "tag_name": "v3", "published_at": "2024-01-04T00:00:00Z", "draft": false },
                    { "tag_name": "v1", "published_at": "2024-01-01T00:00:00Z", "draft": false },
                    { "tag_name": "v2", "published_at": "2024-01-03T00:00:00Z", "draft": false }
                ]]),
            ),
            (
                "repos/o/r/compare/v1...v2?per_page=100".into(),
                json!([{ "commits": [] }]),
            ),
            (
                "repos/o/r/compare/v2...v3?per_page=100".into(),
                json!([{ "commits": [{ "commit": { "message": "The change (#42)" } }] }]),
            ),
        ]));

        assert_eq!(lookup_release_for(&github, "o/r", 42).unwrap().unwrap().tag, "v3");
    }

    #[test]
    fn errors_when_no_release_predates_the_pr() {
        let github = FakeGithub(HashMap::from([
            (
                "repos/o/r/pulls/42".into(),
                json!({ "created_at": "2023-01-01T00:00:00Z" }),
            ),
            (
                "repos/clockworklabs/SpacetimeDB/releases?per_page=100".into(),
                json!([[
                    { "tag_name": "v1", "published_at": "2024-01-01T00:00:00Z", "draft": false }
                ]]),
            ),
        ]));

        let error = lookup_release_for(&github, "o/r", 42).unwrap_err();
        assert!(error.to_string().contains("no published release predates o/r#42"));
    }

    #[test]
    fn matches_only_subject_suffix() {
        assert!(subject_matches("Change thing (#42)\n\nBody", "(#42)"));
        assert!(!subject_matches("Change thing (#42) extra", "(#42)"));
    }
}
