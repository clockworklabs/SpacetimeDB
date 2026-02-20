//! Lightweight update notice check for the proxy path.
//!
//! Before exec'ing the CLI, we check a cache file to see if a newer version
//! is available. If the cache is stale (>24h), we do a quick HTTP check with
//! a short timeout to refresh it. The notice is printed to stderr.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

/// Fetch the latest release version tag from GitHub, using the same release URL
/// infrastructure as `spacetime version upgrade`.
async fn fetch_latest_version(client: &reqwest::Client) -> Option<String> {
    let releases_url = std::env::var("SPACETIME_UPDATE_RELEASES_URL")
        .unwrap_or_else(|_| "https://api.github.com/repos/clockworklabs/SpacetimeDB/releases".to_owned());
    let url = format!("{releases_url}/latest");

    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
    }

    let release: Release = resp.json().await.ok()?;
    let version = release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name);
    Some(version.to_owned())
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

    if now.saturating_sub(cache.last_check_secs) < CHECK_INTERVAL.as_secs() {
        // Cache is fresh — just check the cached version.
        if let Some(ref latest_str) = cache.latest_version {
            if let Ok(latest) = semver::Version::parse(latest_str) {
                if latest > current {
                    print_notice(&current, &latest);
                }
            }
        }
        return Some(());
    }

    // Cache is stale — do a quick network check.
    let latest_str = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?
        .block_on(async {
            let client = reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .user_agent(format!("SpacetimeDB CLI/{CURRENT_VERSION}"))
                .build()
                .ok()?;
            fetch_latest_version(&client).await
        });

    // Update the cache regardless of whether we got a result.
    let new_cache = Cache {
        last_check_secs: now,
        latest_version: latest_str.clone(),
    };
    write_cache(&path, &new_cache);

    if let Some(ref latest_str) = latest_str {
        if let Ok(latest) = semver::Version::parse(latest_str) {
            if latest > current {
                print_notice(&current, &latest);
            }
        }
    }

    Some(())
}

#[allow(clippy::disallowed_macros)]
fn print_notice(current: &semver::Version, latest: &semver::Version) {
    eprintln!("\x1b[33mA new version of SpacetimeDB is available: v{latest} (current: v{current})\x1b[0m");
    eprintln!("Run `spacetime version upgrade` to update.");
    eprintln!();
}
