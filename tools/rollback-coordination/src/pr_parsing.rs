use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use std::collections::BTreeSet;
use std::sync::OnceLock;

const ROLLBACK_SAFETY_HEADING: &str = "Rollback safety impact";

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct PullRequestRef {
    // fully-qualified repo name in owner/repo format
    pub(crate) repo: String,
    pub(crate) number: u64,
}

pub(crate) fn rollback_dependencies(
    body: &str,
    current_repo: &str,
    // If specified, do extra strict verification against this default PR template
    verify_by_strict_template: Option<&str>,
) -> Result<Option<BTreeSet<PullRequestRef>>> {
    let Some(section) = rollback_safety_section(body) else {
        if verify_by_strict_template.is_some() {
            bail!("PR description is missing the `{ROLLBACK_SAFETY_HEADING}` section");
        }
        return Ok(None);
    };
    let dependencies = references(section, current_repo)?;

    if let Some(template) = verify_by_strict_template {
        let template_section = rollback_safety_section(template)
            .context("pull request template is missing the `{ROLLBACK_SAFETY_HEADING}` section")?;
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

    #[test]
    fn extracts_and_validates_rollback_dependencies() {
        const REPO: &str = "clockworklabs/SpacetimeDB";
        let template = "# Rollback safety impact\n\n<!-- instructions -->\n";
        assert_eq!(rollback_dependencies("", REPO, None).unwrap(), None);
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
    }

    #[test]
    fn recognizes_references_and_ignores_comments_and_fences() {
        let section =
            "#12 SpacetimeDBPrivate#13 other/repo#14 https://github.com/x/y/pull/15\n<!-- #16 -->\n```\n#17\n```";
        let refs = references(section, "clockworklabs/SpacetimeDB").unwrap();
        assert_eq!(refs.len(), 4);
        assert!(refs.contains(&PullRequestRef {
            repo: "x/y".into(),
            number: 15
        }));
    }
}
