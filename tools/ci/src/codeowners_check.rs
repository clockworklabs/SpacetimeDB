use anyhow::{anyhow, bail, Context, Result};
use duct::cmd;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::sync::OnceLock;

const REQUIRED_LICENSE_REVIEWER: &str = "cloutiertyler";

pub fn run(pr_number: Option<u64>) -> Result<()> {
    super::ensure_repo_root()?;

    let base_ref = base_ref()?;
    fetch_base_ref(&base_ref)?;

    let license_files = changed_license_files(&base_ref)?;
    if license_files.is_empty() {
        println!("No LICENSE files changed.");
        return Ok(());
    }

    let disallowed_files = disallowed_license_changes(&base_ref, &license_files)?;
    if disallowed_files.is_empty() {
        println!("LICENSE changes are limited to version numbers and change dates.");
        return Ok(());
    }

    let pr_number = pr_number.or_else(pr_number_from_env).ok_or_else(|| {
        anyhow!(
            "LICENSE files have non-version/date changes, but no pull request number was provided. \
             Re-run with --pr-number <number> or set GitHub Actions pull request context."
        )
    })?;

    if has_required_approval(pr_number)? {
        println!("LICENSE changes approved by {REQUIRED_LICENSE_REVIEWER} on PR #{pr_number}.");
        return Ok(());
    }

    bail!(
        "LICENSE files have changes beyond version numbers and change dates, and PR #{pr_number} \
         does not have an approval from {REQUIRED_LICENSE_REVIEWER}: {}",
        disallowed_files
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

fn base_ref() -> Result<String> {
    if let Ok(base_ref) = env::var("GITHUB_BASE_REF") {
        if !base_ref.is_empty() {
            return Ok(format!("origin/{base_ref}"));
        }
    }

    if let Ok(event_path) = env::var("GITHUB_EVENT_PATH") {
        let event = std::fs::read_to_string(event_path)?;
        let event: Value = serde_json::from_str(&event)?;
        if let Some(base_ref) = event
            .pointer("/pull_request/base/ref")
            .and_then(Value::as_str)
            .filter(|base_ref| !base_ref.is_empty())
        {
            return Ok(format!("origin/{base_ref}"));
        }
    }

    Ok("origin/master".to_string())
}

fn fetch_base_ref(base_ref: &str) -> Result<()> {
    let Some(ref_name) = base_ref.strip_prefix("origin/") else {
        return Ok(());
    };
    cmd!(
        "git",
        "fetch",
        "--no-tags",
        "--depth=1",
        "origin",
        &format!("{ref_name}:refs/remotes/origin/{ref_name}")
    )
    .run()
    .with_context(|| format!("failed to fetch base ref {base_ref}"))?;
    Ok(())
}

fn changed_license_files(base_ref: &str) -> Result<Vec<std::path::PathBuf>> {
    let output = cmd!("git", "diff", "--name-only", &format!("{base_ref}...HEAD"))
        .read()
        .with_context(|| format!("failed to list changed files against {base_ref}"))?;
    Ok(output
        .lines()
        .map(Path::new)
        .filter(|path| is_license_file(path))
        .map(Path::to_path_buf)
        .collect())
}

fn disallowed_license_changes(base_ref: &str, license_files: &[std::path::PathBuf]) -> Result<Vec<std::path::PathBuf>> {
    let mut disallowed = Vec::new();
    for path in license_files {
        let diff = cmd!(
            "git",
            "diff",
            "--unified=0",
            "--no-ext-diff",
            &format!("{base_ref}...HEAD"),
            "--",
            path
        )
        .read()
        .with_context(|| format!("failed to read diff for {}", path.display()))?;
        if !diff_only_changes_license_version_or_date(&diff) {
            disallowed.push(path.clone());
        }
    }
    Ok(disallowed)
}

fn is_license_file(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    if path_str.starts_with("licenses/") {
        return true;
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().starts_with("license"))
}

fn diff_only_changes_license_version_or_date(diff: &str) -> bool {
    let mut pending_removal: Option<&str> = None;
    let mut saw_change = false;

    for line in diff.lines() {
        if line.starts_with("diff --git ")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("@@")
        {
            continue;
        }

        if let Some(removed) = line.strip_prefix('-') {
            if pending_removal.replace(removed).is_some() {
                return false;
            }
            saw_change = true;
            continue;
        }

        if let Some(added) = line.strip_prefix('+') {
            let Some(removed) = pending_removal.take() else {
                return false;
            };
            if !allowed_license_line_change(removed, added) {
                return false;
            }
            saw_change = true;
            continue;
        }

        if pending_removal.is_some() {
            return false;
        }
    }

    pending_removal.is_none() && saw_change
}

fn allowed_license_line_change(old: &str, new: &str) -> bool {
    normalized_license_version_line(old) == normalized_license_version_line(new)
        && normalized_license_version_line(old) != old
        || normalized_change_date_line(old) == normalized_change_date_line(new)
            && normalized_change_date_line(old) != old
}

fn normalized_license_version_line(line: &str) -> String {
    static VERSION: OnceLock<Regex> = OnceLock::new();
    VERSION
        .get_or_init(|| Regex::new(r"\bSpacetimeDB \d+\.\d+\.\d+\b").unwrap())
        .replace_all(line, "SpacetimeDB <version>")
        .into_owned()
}

fn normalized_change_date_line(line: &str) -> String {
    static CHANGE_DATE: OnceLock<Regex> = OnceLock::new();
    CHANGE_DATE
        .get_or_init(|| Regex::new(r"\bChange Date:\s+\d{4}-\d{2}-\d{2}\b").unwrap())
        .replace_all(line, "Change Date: <date>")
        .into_owned()
}

fn pr_number_from_env() -> Option<u64> {
    if let Ok(event_path) = env::var("GITHUB_EVENT_PATH") {
        let event = std::fs::read_to_string(event_path).ok()?;
        let event: Value = serde_json::from_str(&event).ok()?;
        if let Some(number) = event.pointer("/pull_request/number").and_then(Value::as_u64) {
            return Some(number);
        }
        if let Some(number) = event.pointer("/number").and_then(Value::as_u64) {
            return Some(number);
        }
        if let Some(number) = event
            .pointer("/inputs/pr_number")
            .and_then(Value::as_str)
            .and_then(|number| number.parse().ok())
        {
            return Some(number);
        }
        if let Some(number) = event
            .pointer("/merge_group/head_ref")
            .and_then(Value::as_str)
            .and_then(parse_pr_number_from_merge_group_ref)
        {
            return Some(number);
        }
    }

    for env_var in ["GITHUB_REF_NAME", "GITHUB_REF"] {
        let Ok(value) = env::var(env_var) else {
            continue;
        };
        if let Some(number) = parse_pr_number_from_ref(&value) {
            return Some(number);
        }
    }

    None
}

fn parse_pr_number_from_ref(value: &str) -> Option<u64> {
    static PR_REF: OnceLock<Regex> = OnceLock::new();
    PR_REF
        .get_or_init(|| Regex::new(r"(?:refs/pull/|^)(\d+)/(?:merge|head)$").unwrap())
        .captures(value)
        .and_then(|captures| captures.get(1))
        .and_then(|number| number.as_str().parse().ok())
}

fn parse_pr_number_from_merge_group_ref(value: &str) -> Option<u64> {
    static MERGE_GROUP_PR_REF: OnceLock<Regex> = OnceLock::new();
    MERGE_GROUP_PR_REF
        .get_or_init(|| Regex::new(r"/pr-(\d+)-").unwrap())
        .captures(value)
        .and_then(|captures| captures.get(1))
        .and_then(|number| number.as_str().parse().ok())
}

fn has_required_approval(pr_number: u64) -> Result<bool> {
    let repo = env::var("GITHUB_REPOSITORY").unwrap_or_else(|_| "clockworklabs/SpacetimeDB".to_string());
    let reviews_json = cmd!(
        "gh",
        "api",
        &format!("repos/{repo}/pulls/{pr_number}/reviews?per_page=100")
    )
    .read()
    .with_context(|| format!("failed to read reviews for PR #{pr_number}"))?;
    let reviews: Value = serde_json::from_str(&reviews_json)?;
    let reviews = reviews
        .as_array()
        .ok_or_else(|| anyhow!("GitHub reviews response was not an array"))?;

    let mut latest_review_by_user = HashMap::new();
    for review in reviews {
        let Some(login) = review.pointer("/user/login").and_then(Value::as_str) else {
            continue;
        };
        let Some(state) = review.get("state").and_then(Value::as_str) else {
            continue;
        };
        latest_review_by_user.insert(login, state);
    }

    Ok(latest_review_by_user
        .get(REQUIRED_LICENSE_REVIEWER)
        .is_some_and(|state| *state == "APPROVED"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_version_and_change_date_updates() {
        let diff = "\
diff --git a/LICENSE.txt b/LICENSE.txt
index 111..222 100644
--- a/LICENSE.txt
+++ b/LICENSE.txt
@@ -8 +8 @@ Licensor:             Clockwork Laboratories, Inc.
-Licensed Work:        SpacetimeDB 2.3.0
+Licensed Work:        SpacetimeDB 2.4.0
@@ -24 +24 @@ Additional Use Grant: You may make use of the Licensed Work provided your
-Change Date:          2031-05-26
+Change Date:          2031-06-01
";

        assert!(diff_only_changes_license_version_or_date(diff));
    }

    #[test]
    fn rejects_unpaired_license_addition() {
        let diff = "\
diff --git a/LICENSE.txt b/LICENSE.txt
--- a/LICENSE.txt
+++ b/LICENSE.txt
@@ -1,0 +2 @@
+New license term
";

        assert!(!diff_only_changes_license_version_or_date(diff));
    }

    #[test]
    fn rejects_non_version_license_edit() {
        let diff = "\
diff --git a/LICENSE.txt b/LICENSE.txt
--- a/LICENSE.txt
+++ b/LICENSE.txt
@@ -1 +1 @@
-Old license term
+New license term
";

        assert!(!diff_only_changes_license_version_or_date(diff));
    }

    #[test]
    fn detects_license_files() {
        assert!(is_license_file(Path::new("LICENSE.txt")));
        assert!(is_license_file(Path::new("crates/cli/LICENSE")));
        assert!(is_license_file(Path::new("licenses/BSL.txt")));
        assert!(!is_license_file(Path::new("crates/cli/Cargo.toml")));
    }

    #[test]
    fn parses_pr_refs() {
        assert_eq!(parse_pr_number_from_ref("refs/pull/123/merge"), Some(123));
        assert_eq!(parse_pr_number_from_ref("456/head"), Some(456));
        assert_eq!(parse_pr_number_from_ref("master"), None);
        assert_eq!(
            parse_pr_number_from_merge_group_ref("gh-readonly-queue/master/pr-789-abcdef"),
            Some(789)
        );
    }
}
