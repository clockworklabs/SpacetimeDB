//! Lightweight update notice check for the proxy path.
//!
//! Before exec'ing the CLI, we check a cache file to see if a newer version
//! is available. If the cache is stale (>24h), we do a quick HTTP check with
//! a short timeout to refresh it. The notice is printed to stderr.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use colored::Colorize;

use crate::cli::install::fetch_latest_release_version;

/// How long to cache the update check result.
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// How often to show the user the same update notice.
const NOTICE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(2);

/// Cache file name.
const CACHE_FILENAME: &str = ".update_check_cache";

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Cache {
    /// Unix timestamp of the last successful check.
    last_check_secs: u64,
    /// The latest version string (without "v" prefix).
    latest_version: String,
    /// Unix timestamp of the last printed update notice.
    #[serde(default)]
    last_notice_secs: u64,
    /// The latest version from the last printed update notice.
    #[serde(default)]
    notice_latest_version: String,
}

impl Cache {
    fn read(config_dir: &Path) -> Option<Self> {
        let contents = std::fs::read_to_string(Self::path(config_dir)).ok()?;
        serde_json::from_str(&contents).ok()
    }

    fn write(&self, config_dir: &Path) {
        if let Ok(json) = serde_json::to_string(self) {
            let _ = std::fs::write(Self::path(config_dir), json);
        }
    }

    fn path(config_dir: &Path) -> PathBuf {
        config_dir.join(CACHE_FILENAME)
    }

    fn should_print_notice(&self, latest: &semver::Version, now: u64) -> bool {
        self.notice_latest_version != latest.to_string()
            || now.saturating_sub(self.last_notice_secs) >= NOTICE_INTERVAL.as_secs()
    }

    fn mark_notice_printed(&mut self, latest: &semver::Version, now: u64) {
        self.last_notice_secs = now;
        self.notice_latest_version = latest.to_string();
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Resolve the latest version, using the cache if fresh or fetching from the network.
///
/// On success, updates the cache. On network failure, leaves the cache unchanged
/// so we retry on the next invocation.
fn latest_version_or_cached(config_dir: &Path) -> Option<semver::Version> {
    let cache = Cache::read(config_dir);
    let now = now_secs();

    // Cache is fresh — use it.
    if let Some(ref cache) = cache
        && now.saturating_sub(cache.last_check_secs) < CHECK_INTERVAL.as_secs()
    {
        return semver::Version::parse(&cache.latest_version).ok();
    }

    // Cache is stale or missing — fetch from network.
    let client = crate::cli::reqwest_client_builder()
        .timeout(UPDATE_CHECK_TIMEOUT)
        .build()
        .ok()?;

    let latest = crate::cli::tokio_block_on(async { fetch_latest_release_version(&client).await }).flatten();

    match latest {
        Ok(version) => {
            Cache {
                last_check_secs: now,
                latest_version: version.to_string(),
                ..Default::default()
            }
            .write(config_dir);
            Some(version)
        }
        Err(e) => {
            log::debug!("Failed to fetch latest version from network; will retry next invocation: {e}");
            // Don't update cache — retry next time.
            // Fall back to stale cache if available.
            cache.and_then(|c| semver::Version::parse(&c.latest_version).ok())
        }
    }
}

/// Check for updates and print a notice to stderr if a newer version is available.
///
/// This is designed to be called from the proxy path before exec'ing the CLI.
/// It reads a cache file to avoid hitting the network on every invocation.
/// If the cache is stale, it makes a quick HTTP request (with timeout) to refresh.
///
/// `config_dir` should be the SpacetimeDB config directory (e.g. `~/.spacetime`).
#[allow(clippy::disallowed_macros)]
pub(crate) fn maybe_print_update_notice(config_dir: &Path) {
    let current = env!("CARGO_PKG_VERSION");
    let current = match semver::Version::parse(current) {
        Ok(v) => v,
        Err(e) => {
            log::debug!("Failed to parse current version: {e}");
            return;
        }
    };

    let latest = match latest_version_or_cached(config_dir) {
        Some(v) => v,
        None => return,
    };

    if latest > current {
        let now = now_secs();
        let mut cache = Cache::read(config_dir).unwrap_or_default();
        if !cache.should_print_notice(&latest, now) {
            return;
        }

        eprintln!(
            "{}",
            format!("A new version of SpacetimeDB is available: v{latest} (current: v{current})").yellow()
        );
        eprintln!("Run `spacetime version upgrade` to update.");
        eprintln!();

        cache.mark_notice_printed(&latest, now);
        if cache.latest_version.is_empty() {
            cache.latest_version = latest.to_string();
        }
        cache.write(config_dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(version: &str) -> semver::Version {
        semver::Version::parse(version).unwrap()
    }

    #[test]
    fn update_notice_prints_when_never_shown() {
        let cache = Cache {
            latest_version: "2.0.0".to_string(),
            ..Default::default()
        };

        assert!(cache.should_print_notice(&version("2.0.0"), 100));
    }

    #[test]
    fn update_notice_is_suppressed_within_interval_for_same_versions() {
        let mut cache = Cache::default();
        let latest = version("2.0.0");
        cache.mark_notice_printed(&latest, 100);

        assert!(!cache.should_print_notice(&latest, 100 + NOTICE_INTERVAL.as_secs() - 1));
    }

    #[test]
    fn update_notice_reprints_after_interval_for_same_versions() {
        let mut cache = Cache::default();
        let latest = version("2.0.0");
        cache.mark_notice_printed(&latest, 100);

        assert!(cache.should_print_notice(&latest, 100 + NOTICE_INTERVAL.as_secs()));
    }

    #[test]
    fn update_notice_reprints_when_latest_version_changes() {
        let mut cache = Cache::default();
        cache.mark_notice_printed(&version("2.0.0"), 100);

        assert!(cache.should_print_notice(&version("2.1.0"), 101));
    }

    #[test]
    fn update_notice_cache_reads_old_format() {
        let tempdir = tempfile::tempdir().unwrap();
        std::fs::write(
            Cache::path(tempdir.path()),
            r#"{"last_check_secs":123,"latest_version":"2.0.0"}"#,
        )
        .unwrap();

        let cache = Cache::read(tempdir.path()).unwrap();
        assert_eq!(cache.last_check_secs, 123);
        assert_eq!(cache.latest_version, "2.0.0");
        assert_eq!(cache.last_notice_secs, 0);
        assert!(cache.notice_latest_version.is_empty());
    }
}
