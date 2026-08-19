use anyhow::{anyhow, bail, Context, Result};
use duct::cmd;
use semver::Version;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub mod gh;
mod pr_parsing;

pub use gh::{Gh, Github};

// This cutoff shortly predates the rollback-safety coordination system, so
// ignoring older tags avoids needless history traversal. It is also after the
// public and private repositories began using the same release tags.
const MINIMUM_RELEASE: Version = Version::new(2, 7, 0);

/// A canonical SpacetimeDB release tag.
///
/// Releases are either `vMAJOR.MINOR.PATCH` or
/// `vMAJOR.MINOR.PATCH-hotfixN`. Their tag is derived from the parsed version;
/// there is no independent string representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Release {
    version: Version,
    hotfix: Option<u64>,
}

impl Release {
    fn from_version(version: Version) -> Result<Self> {
        if !version.build.is_empty() {
            bail!("release versions may not contain build metadata");
        }
        let hotfix = if version.pre.is_empty() {
            None
        } else {
            let prerelease = version.pre.as_str();
            let number = prerelease
                .strip_prefix("hotfix")
                .filter(|number| !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
                .with_context(|| format!("unsupported release suffix `-{prerelease}`; expected `-hotfixN`"))?;
            let parsed = number
                .parse::<u64>()
                .with_context(|| format!("invalid hotfix number `{number}`"))?;
            if parsed.to_string() != number {
                bail!("hotfix number `{number}` is not canonical");
            }
            Some(parsed)
        };
        Ok(Self { version, hotfix })
    }

    fn core(&self) -> (u64, u64, u64) {
        (self.version.major, self.version.minor, self.version.patch)
    }
}

impl FromStr for Release {
    type Err = anyhow::Error;

    fn from_str(tag: &str) -> Result<Self> {
        let version = tag
            .strip_prefix('v')
            .with_context(|| format!("release tag `{tag}` must start with `v`"))?;
        Self::from_version(Version::parse(version).with_context(|| format!("invalid release tag `{tag}`"))?)
    }
}

impl fmt::Display for Release {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "v{}", self.version)
    }
}

impl Ord for Release {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core()
            .cmp(&other.core())
            .then_with(|| self.hotfix.cmp(&other.hotfix))
    }
}

impl PartialOrd for Release {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug)]
struct Repository {
    path: PathBuf,
    releases: Vec<Release>,
}

/// Loads compatible release tags from `path`, newest first.
pub fn load_releases(repo_path: &Path, ignore_incompatible_tags: bool) -> Result<Vec<Release>> {
    let output = cmd!("git", "tag", "--list")
        .dir(repo_path)
        .read()
        .with_context(|| format!("failed to list tags in {}", repo_path.display()))?;
    let mut releases = BTreeSet::new();
    for tag in output.lines().filter(|tag| !tag.is_empty()) {
        let Some(version) = tag.strip_prefix('v') else {
            continue;
        };
        let Ok(version) = Version::parse(version) else {
            continue;
        };
        // Compare only the core version here. SemVer orders prereleases before
        // their base release, whereas our hotfix tags come after it.
        if Version::new(version.major, version.minor, version.patch) < MINIMUM_RELEASE {
            continue;
        }
        match Release::from_version(version) {
            Ok(release) => {
                releases.insert(release);
            }
            Err(error) if ignore_incompatible_tags => {
                tracing::warn!(tag, "Ignoring incompatible release tag: {error:#}");
            }
            Err(error) => return Err(error).with_context(|| format!("incompatible release tag `{tag}`")),
        }
    }
    Ok(releases.into_iter().rev().collect())
}

// Loads repos found a specific paths, and returns a mapping from repo name to repo
fn load_repos(
    github: &impl Github,
    paths: &[&Path],
    ignore_incompatible_tags: bool,
) -> Result<BTreeMap<String, Repository>> {
    let mut repositories = BTreeMap::new();
    for &path in paths {
        let path = path
            .canonicalize()
            .with_context(|| format!("failed to canonicalize repository path {}", path.display()))?;
        let name = github
            .repository_info(&path)
            .with_context(|| format!("failed to identify repository at {}", path.display()))?
            .name_with_owner;
        let releases = load_releases(&path, ignore_incompatible_tags)?;
        repositories.insert(name, Repository { path, releases });
    }
    Ok(repositories)
}

fn lookup_release_for(repository: &Repository, repo: &str, pr_number: u64) -> Result<Option<Release>> {
    tracing::info!("Looking up {repo}#{pr_number}");
    let suffix = format!("(#{pr_number})");
    for pair in repository.releases.windows(2) {
        let [release, base] = pair else { unreachable!() };
        tracing::info!("Checking commits {base}..{release} for release {release}");
        let subjects = cmd!("git", "log", "--format=%s", format!("{base}..{release}"))
            .dir(&repository.path)
            .read()
            .with_context(|| {
                format!(
                    "failed to inspect commits {base}..{release} in {}",
                    repository.path.display()
                )
            })?;
        // N.B. that this is based on commit subject lines, so it can technically be spoofed.
        if subjects.lines().any(|subject| subject.ends_with(&suffix)) {
            tracing::info!("Matched {release}");
            return Ok(Some(release.clone()));
        }
    }
    tracing::info!("No release tag contains {repo}#{pr_number}");
    Ok(None)
}

/// Computes the earliest release to which all requested pull requests can safely be rolled back.
pub fn earliest_rollback_point(
    github: &impl Github,
    current_repo: &Path,
    allowed_reference_repos: &[&Path],
    strict_template: Option<&str>,
    ignore_incompatible_tags: bool,
    pr_numbers: &[u64],
) -> Result<Option<Release>> {
    let repositories = load_repos(github, allowed_reference_repos, ignore_incompatible_tags)?;
    let current_repo_path = current_repo
        .canonicalize()
        .with_context(|| format!("failed to canonicalize repository path {}", current_repo.display()))?;
    let current_repo = github
        .repository_info(&current_repo_path)
        .with_context(|| format!("failed to identify repository at {}", current_repo_path.display()))?
        .name_with_owner;

    let results = pr_numbers
        .iter()
        .map(|&number| earliest_rollback_point_for_pr(github, &repositories, &current_repo, strict_template, number))
        .collect::<Vec<_>>();
    Ok(results.collect_all()?.into_iter().flatten().max())
}

fn earliest_rollback_point_for_pr(
    github: &impl Github,
    repositories: &BTreeMap<String, Repository>,
    current_repo: &str,
    strict_template: Option<&str>,
    number: u64,
) -> Result<Option<Release>> {
    let pr = github
        .pull_request(current_repo, number)
        .with_context(|| format!("failed to load {current_repo}#{number}"))?;
    let body = pr.body.as_deref().unwrap_or_default();
    let Some(dependencies) = pr_parsing::rollback_dependencies(body, current_repo, strict_template)
        .with_context(|| format!("failed to validate {current_repo}#{number}"))?
    else {
        tracing::info!("{current_repo}#{number} has no dependencies");
        return Ok(None);
    };

    let results = dependencies
        .into_iter()
        .map(|dependency| {
            let repository = repositories.get(&dependency.repo).with_context(|| {
                format!(
                    "{}#{} is not in an allowed repository (required by {current_repo}#{number})",
                    dependency.repo, dependency.number
                )
            })?;
            let release = lookup_release_for(repository, &dependency.repo, dependency.number)
                .with_context(|| {
                    format!(
                        "release lookup failed for {}#{} (required by {current_repo}#{number})",
                        dependency.repo, dependency.number
                    )
                })?
                .with_context(|| {
                    format!(
                        "{}#{} has not been released (required by {current_repo}#{number})",
                        dependency.repo, dependency.number
                    )
                })?;
            tracing::info!(
                "{}#{} was released in {} (required by {current_repo}#{number})",
                dependency.repo,
                dependency.number,
                release
            );
            Ok(Some(release))
        })
        .collect::<Vec<Result<Option<Release>>>>();
    Ok(results.collect_all()?.into_iter().flatten().max())
}

trait CollectAll<T> {
    // Collect and combine all errors from a Vec<Result<T>>
    fn collect_all(self) -> Result<Vec<T>>;
}

impl<T> CollectAll<T> for Vec<Result<T>> {
    fn collect_all(self) -> Result<Vec<T>> {
        let mut values = Vec::new();
        let mut errors = Vec::new();

        for result in self {
            match result {
                Ok(value) => values.push(value),
                Err(error) => errors.push(error),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::process::Command;
    use tempfile::TempDir;

    struct FakeGithub {
        names: HashMap<PathBuf, String>,
        responses: HashMap<String, Value>,
    }

    impl Github for FakeGithub {
        fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
            serde_json::from_value(
                self.responses
                    .get(endpoint)
                    .unwrap_or_else(|| panic!("unexpected endpoint {endpoint}"))
                    .clone(),
            )
            .map_err(Into::into)
        }

        fn repository_info(&self, path: &Path) -> Result<gh::RepositoryInfo> {
            let name_with_owner = self
                .names
                .get(path)
                .cloned()
                .with_context(|| format!("unexpected repository path {}", path.display()))?;
            Ok(gh::RepositoryInfo { name_with_owner })
        }
    }

    fn git(path: &Path, args: &[&str]) {
        let status = Command::new("git").args(args).current_dir(path).status().unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn repository(tags: &[&str]) -> TempDir {
        let directory = tempfile::tempdir().unwrap();
        git(directory.path(), &["init", "-q"]);
        git(directory.path(), &["config", "user.name", "Test"]);
        git(directory.path(), &["config", "user.email", "test@example.com"]);
        std::fs::write(directory.path().join("file"), "base").unwrap();
        git(directory.path(), &["add", "file"]);
        git(directory.path(), &["commit", "-qm", "base"]);
        for tag in tags {
            git(directory.path(), &["tag", tag]);
        }
        directory
    }

    fn commit(path: &Path, contents: &str, subject: &str) {
        std::fs::write(path.join("file"), contents).unwrap();
        git(path, &["add", "file"]);
        git(path, &["commit", "-qm", subject]);
    }

    #[test]
    fn release_is_canonical_and_uses_hotfix_ordering() {
        let base: Release = "v2.8.0".parse().unwrap();
        let hotfix_2: Release = "v2.8.0-hotfix2".parse().unwrap();
        let hotfix_10: Release = "v2.8.0-hotfix10".parse().unwrap();
        assert_eq!(base.to_string(), "v2.8.0");
        assert!(base < hotfix_2);
        assert!(hotfix_2 < hotfix_10);
        assert!("2.8.0".parse::<Release>().is_err());
        assert!("v2.8.0-rc1".parse::<Release>().is_err());
        assert!("v2.8.0-hotfix01".parse::<Release>().is_err());
        assert!("v2.8.0+build".parse::<Release>().is_err());
    }

    #[test]
    fn loads_newest_releases_and_applies_cutoff_before_suffix_validation() {
        let repository = repository(&["v2.6.1-bad", "v2.7.0", "v2.7.0-hotfix2", "v2.8.0", "not-a-release"]);
        let releases = load_releases(repository.path(), false).unwrap();
        assert_eq!(
            releases.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["v2.8.0", "v2.7.0-hotfix2", "v2.7.0"]
        );
    }

    #[test]
    fn errors_or_ignores_incompatible_release_tags() {
        let repository = repository(&["v2.7.0", "v2.8.0-rc1"]);
        assert!(load_releases(repository.path(), false).is_err());
        assert_eq!(
            load_releases(repository.path(), true).unwrap(),
            vec!["v2.7.0".parse().unwrap()]
        );
    }

    #[test]
    fn rejects_different_release_sets() {
        let first = repository(&["v2.7.0", "v2.8.0"]);
        let second = repository(&["v2.7.0"]);
        let first_path = first.path().canonicalize().unwrap();
        let second_path = second.path().canonicalize().unwrap();
        let github = FakeGithub {
            names: HashMap::from([(first_path, "o/first".into()), (second_path, "o/second".into())]),
            responses: HashMap::new(),
        };
        let error = load_repos(&github, &[first.path(), second.path()], false).unwrap_err();
        assert!(error.to_string().contains("release tags differ"));
        assert!(error.to_string().contains("v2.8.0"));
    }

    #[test]
    fn finds_a_pr_in_its_exact_adjacent_release_range() {
        let repository = repository(&["v2.7.0"]);
        commit(repository.path(), "change", "The change (#42)");
        git(repository.path(), &["tag", "v2.8.0"]);
        commit(repository.path(), "later", "A later change (#43)");
        git(repository.path(), &["tag", "v2.8.1"]);
        let repository = Repository {
            path: repository.path().to_owned(),
            releases: load_releases(repository.path(), false).unwrap(),
        };
        assert_eq!(
            lookup_release_for(&repository, "o/r", 42).unwrap(),
            Some("v2.8.0".parse().unwrap())
        );
    }

    #[test]
    fn reports_all_unsatisfied_dependencies() {
        let repository = repository(&["v2.7.0", "v2.8.0"]);
        let path = repository.path().canonicalize().unwrap();
        let github = FakeGithub {
            names: HashMap::from([(path, "o/r".into())]),
            responses: HashMap::from([
                (
                    "repos/o/r/pulls/1".into(),
                    json!({ "body": "# Rollback safety impact\n#41" }),
                ),
                (
                    "repos/o/r/pulls/2".into(),
                    json!({ "body": "# Rollback safety impact\n#42" }),
                ),
            ]),
        };
        let error = earliest_rollback_point(&github, repository.path(), &[repository.path()], None, false, &[1, 2])
            .unwrap_err();
        assert!(error.to_string().contains("o/r#41 has not been released"));
        assert!(error.to_string().contains("o/r#42 has not been released"));
    }

    #[test]
    fn collect_all_combines_every_error() {
        let results: Vec<Result<u64>> = vec![Err(anyhow!("first")), Ok(42), Err(anyhow!("second"))];
        let error = results.collect_all().unwrap_err();
        assert_eq!(error.to_string(), "first\nsecond");
    }
}
