use anyhow::{anyhow, bail, Context, Result};
use duct::cmd;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const REPO: &str = "clockworklabs/SpacetimeDB";
const REQUIRED_LICENSE_REVIEWER: &str = "cloutiertyler";

pub fn run(base_ref: &str, pr_number: u64) -> Result<()> {
    super::ensure_repo_root()?;

    fetch_base_ref(base_ref)?;

    let mut review_statuses = ReviewStatuses::new(pr_number);
    for path in changed_files(base_ref)? {
        if is_license_file(&path) {
            if !is_trivial_license_change(base_ref, &path)? {
                require_review_from(&path, &mut review_statuses, REQUIRED_LICENSE_REVIEWER)?;
            }
        }
    }

    Ok(())
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

fn changed_files(base_ref: &str) -> Result<Vec<PathBuf>> {
    let output = cmd!("git", "diff", "--name-only", &format!("{base_ref}...HEAD"))
        .read()
        .with_context(|| format!("failed to list changed files against {base_ref}"))?;
    Ok(output.lines().map(Path::new).map(Path::to_path_buf).collect())
}

fn is_trivial_license_change(base_ref: &str, path: &Path) -> Result<bool> {
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
    Ok(diff_only_changes_license_version_or_date(&diff))
}

struct ReviewStatuses {
    pr_number: u64,
    latest_by_author: Option<HashMap<String, String>>,
}

impl ReviewStatuses {
    fn new(pr_number: u64) -> Self {
        Self {
            pr_number,
            latest_by_author: None,
        }
    }

    fn approved_by(&mut self, reviewer: &str) -> Result<bool> {
        if self.latest_by_author.is_none() {
            self.latest_by_author = Some(latest_review_status_by_author(self.pr_number)?);
        }

        Ok(self
            .latest_by_author
            .as_ref()
            .and_then(|statuses| statuses.get(reviewer))
            .is_some_and(|state| state == "APPROVED"))
    }
}

fn require_review_from(path: &Path, review_statuses: &mut ReviewStatuses, reviewer: &str) -> Result<()> {
    if review_statuses.approved_by(reviewer)? {
        return Ok(());
    }

    bail!(
        "{} has changes beyond version numbers and change dates, and PR #{} \
         does not have an approval from {reviewer}",
        path.display(),
        review_statuses.pr_number
    );
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

fn latest_review_status_by_author(pr_number: u64) -> Result<HashMap<String, String>> {
    let reviews_json = cmd!(
        "gh",
        "api",
        &format!("repos/{REPO}/pulls/{pr_number}/reviews?per_page=100")
    )
    .read()
    .with_context(|| format!("failed to read reviews for PR #{pr_number}"))?;
    let reviews: Value = serde_json::from_str(&reviews_json)?;
    let reviews = reviews
        .as_array()
        .ok_or_else(|| anyhow!("GitHub reviews response was not an array"))?;

    let mut latest_by_author = HashMap::new();
    for review in reviews {
        let Some(login) = review.pointer("/user/login").and_then(Value::as_str) else {
            continue;
        };
        let Some(state) = review.get("state").and_then(Value::as_str) else {
            continue;
        };
        latest_by_author.insert(login.to_string(), state.to_string());
    }

    Ok(latest_by_author)
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
}
