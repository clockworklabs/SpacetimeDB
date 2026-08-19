use anyhow::{bail, Context, Result};
use semver::Version;
use std::cmp::Ordering;
use std::fmt;

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
    pub fn from_tag(tag: &str) -> Result<Option<Self>> {
        let Some(version) = tag.strip_prefix('v') else {
            tracing::debug!("Not parsing tag that does not start with `v`");
            return Ok(None);
        };
        let Ok(version) = Version::parse(version) else {
            tracing::debug!("Not parsing tag that is not a semver");
            return Ok(None);
        };

        // Ignore releases that are too old.
        // This cutoff shortly predates the rollback-safety coordination system, so
        // ignoring older tags avoids needless history traversal. It is also after the
        // public and private repositories began using the same release tags.
        // We compare only the core version here. SemVer orders prereleases before
        // their base release, whereas our hotfix tags come after it.
        if Version::new(version.major, version.minor, version.patch) < Version::new(2, 7, 0) {
            tracing::debug!("Not parsing tag that is too old");
            return Ok(None);
        }

        if !version.build.is_empty() {
            bail!("release versions may not contain build metadata");
        }

        let hotfix = if version.pre.is_empty() {
            None
        } else {
            let prerelease = version.pre.as_str();
            let number = prerelease
                .strip_prefix("hotfix")
                .and_then(|number| number.parse::<u64>().ok())
                .with_context(|| format!("unsupported release suffix `-{prerelease}`; expected `-hotfixN`"))?;
            // Reject things that start with 0, include separators, etc.
            if format!("hotfix{number}") != prerelease {
                bail!("release suffix `-{prerelease}` is not a canonical form");
            }
            Some(number)
        };

        Ok(Some(Self { version, hotfix }))
    }
}

impl fmt::Display for Release {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "v{}", self.version)
    }
}

impl Ord for Release {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.version.major, self.version.minor, self.version.patch)
            .cmp(&(other.version.major, other.version.minor, other.version.patch))
            .then_with(|| self.hotfix.cmp(&other.hotfix))
    }
}

impl PartialOrd for Release {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
