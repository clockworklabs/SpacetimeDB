//! Lightweight update notice check for the proxy path.
//!
//! Before exec'ing the CLI, we check a cache file to see if a newer version
//! is available. If the cache is stale (>24h), we do a quick HTTP check with
//! a short timeout to refresh it. The notice is printed to stderr.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::cli::install::fetch_latest_release_version;

/// How long to cache the update check result.
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// HTTP timeout for the version check.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Cache file name.
const CACHE_FILENAME: &str = ".update_check_cache";

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Cache {
    /// Unix timestamp of the last successful check.
    last_check_secs: u64,
    /// The latest version string (without "v" prefix).
    latest_version: String,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn cache_path(config_dir: &Path) -> PathBuf {
    config_dir.join(CACHE_FILENAME)
}

fn read_cache(path: &Path) -> Option<Cache> {
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_cache(path: &Path, cache: &Cache) {
    if let Ok(json) = serde_json::to_string(cache) {
        let _ = std::fs::write(path, json);
    }
}

/// Resolve the latest version, using the cache if fresh or fetching from the network.
///
/// On success, updates the cache. On network failure, leaves the cache unchanged
/// so we retry on the next invocation.
fn resolve_latest_version(config_dir: &Path) -> Option<semver::Version> {
    let path = cache_path(config_dir);
    let cache = read_cache(&path);
    let now = now_secs();

    // Cache is fresh — use it.
    if let Some(ref cache) = cache {
        if now.saturating_sub(cache.last_check_secs) < CHECK_INTERVAL.as_secs() {
            return semver::Version::parse(&cache.latest_version).ok();
        }
    }

    // Cache is stale or missing — fetch from network.
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(format!("SpacetimeDB CLI/{CURRENT_VERSION}"))
        .build()
        .ok()?;

    let latest = crate::cli::tokio_block_on(async { fetch_latest_release_version(&client).await })
        .ok()
        .flatten();

    match latest {
        Some(version) => {
            write_cache(
                &path,
                &Cache {
                    last_check_secs: now,
                    latest_version: version.to_string(),
                },
            );
            Some(version)
        }
        None => {
            log::debug!("Failed to fetch latest version from network; will retry next invocation");
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
pub(crate) fn maybe_print_update_notice(config_dir: &Path) {
    let current = match semver::Version::parse(CURRENT_VERSION) {
        Ok(v) => v,
        Err(_) => return,
    };

    let latest = match resolve_latest_version(config_dir) {
        Some(v) => v,
        None => return,
    };

    if latest > current {
        print_notice(&current, &latest);
    }
}

#[allow(clippy::disallowed_macros)]
fn print_notice(current: &semver::Version, latest: &semver::Version) {
    eprintln!("\x1b[33mA new version of SpacetimeDB is available: v{latest} (current: v{current})\x1b[0m");
    eprintln!("Run `spacetime version upgrade` to update.");
    eprintln!();
}
