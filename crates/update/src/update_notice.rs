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
    /// The latest version string (without "v" prefix), if known.
    latest_version: Option<String>,
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

/// If `latest` is newer than `current`, print an update notice to stderr.
fn notify_if_newer(current: &semver::Version, latest_str: &str) {
    if let Ok(latest) = semver::Version::parse(latest_str) {
        if latest > *current {
            print_notice(current, &latest);
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
    // Best-effort: never let a failed update check interfere with the user's command.
    let _ = check_and_notify(config_dir);
}

fn check_and_notify(config_dir: &Path) -> Option<()> {
    let path = cache_path(config_dir);
    let cache = read_cache(&path).unwrap_or_default();
    let now = now_secs();

    let current = semver::Version::parse(CURRENT_VERSION).ok()?;

    // Cache is fresh and has a known latest version — use it.
    if now.saturating_sub(cache.last_check_secs) < CHECK_INTERVAL.as_secs() {
        if let Some(ref latest_str) = cache.latest_version {
            notify_if_newer(&current, latest_str);
            return Some(());
        }
        // Cache is fresh but latest_version is None (previous fetch failed).
        // Fall through to re-check rather than silently skipping for 24h.
    }

    // Cache is stale or empty — do a quick network check.
    let latest_version = fetch_latest_version_now();

    match latest_version {
        Some(ref latest) => {
            // Successful fetch — save to cache and compare.
            let new_cache = Cache {
                last_check_secs: now,
                latest_version: Some(latest.to_string()),
            };
            write_cache(&path, &new_cache);
            notify_if_newer(&current, &latest.to_string());
        }
        None => {
            // Fetch failed — don't update the cache so we retry next invocation.
        }
    }

    Some(())
}

/// Fetch the latest version from GitHub/mirror with a short timeout.
fn fetch_latest_version_now() -> Option<semver::Version> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?
        .block_on(async {
            let client = reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .user_agent(format!("SpacetimeDB CLI/{CURRENT_VERSION}"))
                .build()
                .ok()?;
            fetch_latest_release_version(&client).await
        })
}

#[allow(clippy::disallowed_macros)]
fn print_notice(current: &semver::Version, latest: &semver::Version) {
    eprintln!("\x1b[33mA new version of SpacetimeDB is available: v{latest} (current: v{current})\x1b[0m");
    eprintln!("Run `spacetime version upgrade` to update.");
    eprintln!();
}
