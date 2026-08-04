use crate::GithubApi;
use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// An open pull request found through a GitHub cross-reference or explicit mention.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RelatedPullRequest {
    pub number: u64,
    pub html_url: String,
}

#[derive(Deserialize)]
struct IssueRepository {
    full_name: Option<String>,
}

#[derive(Deserialize)]
struct Issue {
    number: u64,
    html_url: String,
    state: Option<String>,
    draft: Option<bool>,
    pull_request: Option<Value>,
    repository: Option<IssueRepository>,
}

#[derive(Deserialize)]
struct TimelineSource {
    issue: Option<Issue>,
}

#[derive(Deserialize)]
struct TimelineEvent {
    event: Option<String>,
    source: Option<TimelineSource>,
}

#[derive(Deserialize)]
struct PullRequestText {
    body: Option<String>,
}

#[derive(Deserialize)]
struct IssueComment {
    body: Option<String>,
}

fn pr_references(repo: &str, text: &str) -> BTreeSet<u64> {
    let mut refs = BTreeSet::new();
    for prefix in [
        format!("https://github.com/{repo}/pull/"),
        format!("http://github.com/{repo}/pull/"),
        format!("github.com/{repo}/pull/"),
        format!("{repo}#"),
    ] {
        collect_numbers_after_prefix(text, &prefix, &mut refs);
    }
    refs
}

fn collect_numbers_after_prefix(text: &str, prefix: &str, refs: &mut BTreeSet<u64>) {
    let mut remaining = text;
    while let Some(index) = remaining.find(prefix) {
        let after_prefix = &remaining[index + prefix.len()..];
        let digits_len = after_prefix.bytes().take_while(|byte| byte.is_ascii_digit()).count();
        if digits_len > 0
            && let Ok(number) = after_prefix[..digits_len].parse::<u64>()
        {
            refs.insert(number);
        }
        remaining = &after_prefix[digits_len..];
    }
}

fn related_prs_from_timeline(
    events: impl IntoIterator<Item = TimelineEvent>,
    from_repo: &str,
    include_drafts: bool,
) -> Vec<RelatedPullRequest> {
    events
        .into_iter()
        .filter_map(|event| {
            if event.event.as_deref() != Some("cross-referenced") {
                return None;
            }
            let issue = event.source?.issue?;
            if issue.repository.as_ref()?.full_name.as_deref() != Some(from_repo)
                || issue.state.as_deref() != Some("open")
                || (!include_drafts && issue.draft == Some(true))
                || issue.pull_request.is_none()
            {
                return None;
            }
            Some((issue.number, issue.html_url))
        })
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .map(|(number, html_url)| RelatedPullRequest { number, html_url })
        .collect()
}

/// Finds open pull requests in `from_repo` that cross-reference pull request
/// `pr` in `in_repo`.
pub fn prs_mentioning(
    github: &GithubApi,
    in_repo: &str,
    from_repo: &str,
    pr: u64,
    include_drafts: bool,
) -> Result<Vec<RelatedPullRequest>> {
    let pages: Vec<Vec<TimelineEvent>> = github.get_paginated(&format!("/repos/{in_repo}/issues/{pr}/timeline"))?;
    Ok(related_prs_from_timeline(
        pages.into_iter().flatten(),
        from_repo,
        include_drafts,
    ))
}

/// Finds open pull requests in `in_repo` explicitly mentioned by a pull request
/// body or comment in `from_repo`.
pub fn prs_mentioned(
    github: &GithubApi,
    from_repo: &str,
    in_repo: &str,
    pr_number: u64,
) -> Result<Vec<RelatedPullRequest>> {
    let pr: PullRequestText = github.pull_request_view(from_repo, pr_number, "body")?;
    let comments: Vec<Vec<IssueComment>> =
        github.get_paginated(&format!("/repos/{from_repo}/issues/{pr_number}/comments"))?;

    let mut referenced = BTreeSet::new();
    if let Some(body) = pr.body {
        referenced.extend(pr_references(in_repo, &body));
    }
    for comment in comments.into_iter().flatten() {
        if let Some(body) = comment.body {
            referenced.extend(pr_references(in_repo, &body));
        }
    }

    let mut related = Vec::new();
    for number in referenced {
        let pull = github.pull_request(in_repo, number)?;
        if pull.state == "open" {
            related.push(RelatedPullRequest {
                number: pull.number,
                html_url: format!("https://github.com/{in_repo}/pull/{}", pull.number),
            });
        }
    }
    Ok(related)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pr_references_collects_supported_github_reference_forms() {
        let references = pr_references(
            "clockworklabs/SpacetimeDB",
            "See clockworklabs/SpacetimeDB#12, https://github.com/clockworklabs/SpacetimeDB/pull/34, and http://github.com/clockworklabs/SpacetimeDB/pull/56.",
        );
        assert_eq!(references.into_iter().collect::<Vec<_>>(), vec![12, 34, 56]);
    }

    #[test]
    fn timeline_filter_deduplicates_open_prs_and_obeys_draft_policy() {
        let events = || {
            serde_json::from_value::<Vec<TimelineEvent>>(serde_json::json!([
                {
                    "event": "cross-referenced",
                    "source": {"issue": {
                        "number": 42,
                        "html_url": "https://example.test/42",
                        "state": "open",
                        "draft": true,
                        "pull_request": {},
                        "repository": {"full_name": "clockworklabs/SpacetimeDBPrivate"}
                    }}
                },
                {
                    "event": "cross-referenced",
                    "source": {"issue": {
                        "number": 42,
                        "html_url": "https://example.test/42",
                        "state": "open",
                        "draft": true,
                        "pull_request": {},
                        "repository": {"full_name": "clockworklabs/SpacetimeDBPrivate"}
                    }}
                }
            ]))
            .unwrap()
        };

        assert!(related_prs_from_timeline(events(), "clockworklabs/SpacetimeDBPrivate", false).is_empty());
        assert_eq!(
            related_prs_from_timeline(events(), "clockworklabs/SpacetimeDBPrivate", true),
            vec![RelatedPullRequest {
                number: 42,
                html_url: "https://example.test/42".to_owned(),
            }]
        );
    }
}
