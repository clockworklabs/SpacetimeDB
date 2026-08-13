#![allow(clippy::disallowed_macros)]

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use lookup_pr_release::{lookup, Gh, Github};
use regex::Regex;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::sync::OnceLock;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PullRequestRef {
    repo: String,
    number: u64,
}

#[derive(Parser)]
#[command(about = "Checks that every PR in the `Must be released` section has been released.")]
struct Args {
    #[arg(long)]
    repo: String,
    #[arg(long)]
    pr_number: u64,
}

fn release_section(body: &str) -> Result<&str> {
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

fn normalized_release_section(body: &str) -> Result<String> {
    Ok(release_section(body)?
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned())
}

fn validated_release_section<'a>(body: &'a str, template: &str) -> Result<&'a str> {
    let section = release_section(body)?;
    if normalized_release_section(body)? == normalized_release_section(template)? {
        bail!(
            "the `Must be released` section is unchanged from the pull request template; remove the instructional comment if there are no dependencies"
        );
    }
    Ok(section)
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

#[derive(Deserialize)]
struct PullRequest {
    body: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let github = Gh;
    let pr: PullRequest = github.get(&format!("repos/{}/pulls/{}", args.repo, args.pr_number))?;
    let body = pr.body.as_deref().unwrap_or_default();
    let template = fs::read_to_string(".github/pull_request_template.md")
        .context("failed to read .github/pull_request_template.md")?;
    let section =
        validated_release_section(body, &template).context("failed to validate the `Must be released` section")?;
    let dependencies = references(section, &args.repo)?;
    let mut unreleased = Vec::new();
    let mut errors = Vec::new();

    for dependency in dependencies {
        match lookup(&github, &dependency.repo, dependency.number, false) {
            Ok(Some(release)) => println!(
                "{}#{} was released in {}",
                dependency.repo, dependency.number, release.tag
            ),
            Ok(None) => unreleased.push(format!("{}#{}", dependency.repo, dependency.number)),
            Err(error) => errors.push(format!("{}#{}: {error:#}", dependency.repo, dependency.number)),
        }
    }

    if !unreleased.is_empty() || !errors.is_empty() {
        let mut message = String::new();
        if !unreleased.is_empty() {
            message.push_str("The following required PRs have not been released:\n");
            for dependency in unreleased {
                message.push_str(&format!("- {dependency}\n"));
            }
        }
        if !errors.is_empty() {
            message.push_str("Release lookup failed for:\n");
            for error in errors {
                message.push_str(&format!("- {error}\n"));
            }
        }
        bail!(message.trim_end().to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_the_release_section() {
        let body = "# Intro\n# Must be released\n#12 repo#13 owner/repo#14\n## Detail\n#15\n# Testing\n#16\n";
        assert_eq!(
            release_section(body).unwrap(),
            "#12 repo#13 owner/repo#14\n## Detail\n#15\n"
        );
    }

    #[test]
    fn normalizes_release_section_whitespace() {
        let body = "# Must be released\r\n\r\n<!-- default -->   \r\n\r\n# Next\r\n";
        assert_eq!(normalized_release_section(body).unwrap(), "<!-- default -->");
    }

    #[test]
    fn rejects_unchanged_template_but_accepts_an_intentionally_empty_section() {
        let template = "# Must be released\n\n<!-- default instructions -->\n";
        let unchanged = "# Must be released\r\n\r\n<!-- default instructions -->   \r\n";
        let empty = "# Must be released\n";
        assert!(validated_release_section(unchanged, template).is_err());
        assert_eq!(validated_release_section(empty, template).unwrap(), "");
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
}
