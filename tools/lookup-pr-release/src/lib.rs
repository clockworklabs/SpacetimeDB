use anyhow::{Context, Result};

pub mod gh;

pub use gh::{Gh, Github};

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
    let first_older = releases.partition_point(|release| release.published_at >= pr.created_at);
    let candidates_and_base = releases
        .get(..=first_older)
        .with_context(|| format!("no published release predates {repo}#{pr_number}"))?;
    for pair in candidates_and_base.windows(2) {
        let [release, base] = pair else { unreachable!() };
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
    Ok(result)
}

fn subject_matches(message: &str, suffix: &str) -> bool {
    message.lines().next().is_some_and(|subject| subject.ends_with(suffix))
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
    #[ignore = "requires GitHub API access"]
    fn live_pr_5255_was_released_in_v2_6_0() {
        assert_live_release(5255, "v2.6.0");
    }

    #[test]
    #[ignore = "requires GitHub API access"]
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
        assert_eq!(releases.last().unwrap().tag, "too-old");
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
    fn searches_candidate_releases_from_newest_to_oldest() {
        let github = FakeGithub(HashMap::from([
            (
                "repos/o/r/pulls/42".into(),
                json!({ "created_at": "2024-01-02T00:00:00Z" }),
            ),
            (
                "repos/clockworklabs/SpacetimeDB/releases?per_page=100".into(),
                json!([[
                    { "tag_name": "v3", "published_at": "2024-01-04T00:00:00Z", "draft": false },
                    { "tag_name": "v2", "published_at": "2024-01-03T00:00:00Z", "draft": false },
                    { "tag_name": "v1", "published_at": "2024-01-01T00:00:00Z", "draft": false }
                ]]),
            ),
            (
                "repos/o/r/compare/v1...v2?per_page=100".into(),
                json!([{ "commits": [{ "commit": { "message": "The change (#42)" } }] }]),
            ),
            (
                "repos/o/r/compare/v2...v3?per_page=100".into(),
                json!([{ "commits": [] }]),
            ),
        ]));

        assert_eq!(lookup_release_for(&github, "o/r", 42).unwrap().unwrap().tag, "v2");
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
