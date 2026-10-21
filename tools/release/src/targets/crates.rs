use crate::targets::{util, ReleaseTarget};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

const CRATE_OWNERS: &[&str] = &["cloutiertyler", "jdetter", "bfops", "rekhoff", "spacetimedb-devops"];
const CRATES_IO_API: &str = "https://crates.io/api/v1";
const MAX_PUBLISH_RETRIES: u32 = 5;

#[derive(Debug, PartialEq, Eq)]
enum PublishOutcome {
    Complete,
    RateLimited,
}

fn run_publish(cmd: &mut Command, dry_run: bool) -> Result<PublishOutcome, String> {
    util::print_command(cmd);
    let mut child = cmd
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Failed to execute cargo publish: {err}"))?;
    let mut rate_limited = false;
    // Keep Cargo's progress visible while watching for the registry's HTTP 429 response.
    let read_result = BufReader::new(child.stderr.take().unwrap())
        .lines()
        .try_for_each(|line| {
            let line = line?;
            eprintln!("{line}");
            rate_limited |= line.contains("429 Too Many Requests");
            Ok::<_, std::io::Error>(())
        });
    let status = child
        .wait()
        .map_err(|err| format!("Failed to wait for cargo publish: {err}"))?;
    read_result.map_err(|err| format!("Failed to read cargo publish output: {err}"))?;
    if status.success() {
        Ok(PublishOutcome::Complete)
    } else if rate_limited && !dry_run {
        Ok(PublishOutcome::RateLimited)
    } else {
        Err(format!("cargo publish failed: {status}"))
    }
}

fn publish_with_retries(
    crates: &[String],
    mut is_published: impl FnMut(&str) -> Result<bool, String>,
    mut publish: impl FnMut(&[&String]) -> Result<PublishOutcome, String>,
    mut sleep: impl FnMut(Duration),
) -> Result<(), String> {
    let mut retries = 0;
    let mut previous_remaining = None;
    loop {
        let mut unpublished = Vec::new();
        println!("Checking for already-published crates..");
        for crate_name in crates {
            if is_published(crate_name)? {
                println!("Crate {crate_name} already published; skipping.");
            } else {
                println!("Crate {crate_name} must be published.");
                unpublished.push(crate_name);
            }
        }
        // Only count retries without progress. Check again even after the last allowed
        // retry, since that attempt may have uploaded more crates before hitting the limit.
        if let Some(previous) = previous_remaining {
            if unpublished.len() < previous {
                retries = 0;
            }
            if !unpublished.is_empty() {
                if retries == MAX_PUBLISH_RETRIES {
                    return Err(format!(
                        "cargo publish still rate limited after {MAX_PUBLISH_RETRIES} retries without progress"
                    ));
                }
                retries += 1;
                // crates.io allows one new version of an existing crate per minute after the initial burst.
                // https://crates.io/docs/rate-limits
                let delay = Duration::from_secs(60);
                println!(
                    "crates.io rate limited cargo publish; retrying in {} seconds ({retries}/{MAX_PUBLISH_RETRIES} without progress).",
                    delay.as_secs(),
                );
                sleep(delay);
            }
        }
        previous_remaining = Some(unpublished.len());
        match publish(&unpublished)? {
            PublishOutcome::Complete => return Ok(()),
            PublishOutcome::RateLimited => continue,
        }
    }
}

pub struct CratesRelease {
    pub version: String,
    pub dry_run: bool,
}

impl CratesRelease {
    pub fn new(version: String, dry_run: bool) -> Self {
        Self { version, dry_run }
    }

    fn publish_crates(&self, workspace_root: &Path, crates: &[&String]) -> Result<PublishOutcome, String> {
        if crates.is_empty() {
            println!("All selected crates are already published; nothing to publish.");
            return Ok(PublishOutcome::Complete);
        }
        // Workaround for some weird bug where dry runs fail if you don't run them in a sterile home dir
        let cargo_home = if self.dry_run {
            Some(tempfile::tempdir().map_err(|err| format!("Failed to create temporary Cargo home: {err}"))?)
        } else {
            None
        };
        // NOTE: We must keep all crates in one publish invocation.
        // Without this, dry-run doesn't work properly: every crate will look for its deps on crates.io
        // and fail, but if they're on one invocation then they're able to resolve properly.
        let mut cmd = Command::new("cargo");
        cmd.current_dir(workspace_root).args(["publish", "--allow-dirty"]);
        for crate_name in crates {
            cmd.args(["-p", crate_name]);
        }
        if self.dry_run {
            cmd.arg("--dry-run");
        }
        if let Some(home) = &cargo_home {
            cmd.env("CARGO_HOME", home.path());
        }
        run_publish(&mut cmd, self.dry_run)
    }

    fn add_crate_owners(&self, crate_name: &str) -> Result<(), String> {
        println!("Adding owners for crate: {}", crate_name);

        for owner in CRATE_OWNERS {
            let mut cmd = Command::new("cargo");
            cmd.args(["owner", "--add", owner, crate_name]);
            util::print_command(&cmd);

            let output = cmd
                .output()
                .map_err(|e| format!("Failed to execute cargo owner --add {} {}: {}", owner, crate_name, e))?;

            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            if output.status.success() {
                println!("Added {} as an owner of {}.", owner, crate_name);
                continue;
            }

            if stderr.contains("already") || stdout.contains("already") {
                println!("{} is already an owner of {}.", owner, crate_name);
                continue;
            }

            return Err(format!(
                "Failed to add owner {} to crate {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
                owner, crate_name, stdout, stderr
            ));
        }

        Ok(())
    }
}

fn is_published(client: &Client, api: &str, crate_name: &str, version: &str) -> Result<bool, String> {
    let url = format!("{}/crates/{crate_name}/{version}", api.trim_end_matches('/'));
    let response = client
        .get(&url)
        .send()
        .map_err(|err| format!("Failed to query {url}: {err}"))?;
    match response.status() {
        StatusCode::OK => Ok(true),
        StatusCode::NOT_FOUND => Ok(false),
        status => Err(format!("Failed to query {url}: {status}")),
    }
}

impl ReleaseTarget for CratesRelease {
    fn release(&self) -> Result<(), String> {
        println!("Finding crates to publish to crates.io...");

        let workspace_root = crate::crates_resolver::find_workspace_root()
            .map_err(|e| format!("Failed to find workspace root: {}", e))?;

        let crates = crate::crates_resolver::get_crates_to_publish()
            .map_err(|e| format!("Failed to find crates to publish: {}", e))?;

        println!("\nCrates to publish in order:");
        for crate_name in &crates {
            println!("  - {}", crate_name);
        }

        if self.dry_run {
            println!("\nDRY RUN: No crates will be published");
        }

        println!("\nStarting publish process...");
        if self.dry_run {
            self.publish_crates(&workspace_root, &crates.iter().collect::<Vec<_>>())?;
        } else {
            let client = Client::builder()
                .user_agent("spacetimedb-release (https://github.com/clockworklabs/SpacetimeDB)")
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|err| format!("Failed to create crates.io API client: {err}"))?;
            let crates_io_version = self
                .version
                .strip_prefix('v')
                .unwrap_or(&self.version)
                .split('-')
                .next()
                .unwrap();
            // Recheck after every rate limit: Cargo may have uploaded part of the batch.
            publish_with_retries(
                &crates,
                |crate_name| {
                    // Pace requests to respect crates.io's API guidance.
                    std::thread::sleep(Duration::from_secs(1));
                    is_published(&client, CRATES_IO_API, crate_name, crates_io_version)
                },
                |unpublished| self.publish_crates(&workspace_root, unpublished),
                std::thread::sleep,
            )?;
        }
        if !self.dry_run {
            for crate_name in &crates {
                self.add_crate_owners(crate_name)?;
            }
        }

        if self.dry_run {
            println!("\nDry run completed successfully!");
        } else {
            println!("\nAll crates published successfully!");
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "crates"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_only_remaining_crates_after_partial_publish() {
        let crates = vec!["first".to_owned(), "second".to_owned()];
        let published = std::cell::RefCell::new(Vec::<String>::new());
        let mut batches = Vec::new();
        let mut delays = Vec::new();
        publish_with_retries(
            &crates,
            |name| Ok(published.borrow().iter().any(|p| p == name)),
            |remaining| {
                batches.push(remaining.iter().map(|name| name.to_string()).collect::<Vec<_>>());
                if batches.len() == 1 {
                    published.borrow_mut().push("first".into());
                    Ok(PublishOutcome::RateLimited)
                } else {
                    Ok(PublishOutcome::Complete)
                }
            },
            |delay| delays.push(delay.as_secs()),
        )
        .unwrap();
        assert_eq!(batches, vec![vec!["first", "second"], vec!["second"]]);
        assert_eq!(delays, vec![60]);
    }

    #[test]
    fn persistent_rate_limit_retries_at_one_minute_intervals() {
        let mut attempts = 0;
        let mut delays = Vec::new();
        let err = publish_with_retries(
            &["crate".into()],
            |_| Ok(false),
            |_| {
                attempts += 1;
                Ok(PublishOutcome::RateLimited)
            },
            |delay| delays.push(delay.as_secs()),
        )
        .unwrap_err();
        assert!(err.contains("still rate limited"));
        assert_eq!(attempts, 6);
        assert_eq!(delays, vec![60, 60, 60, 60, 60]);
    }

    #[test]
    fn progress_on_last_retry_resets_the_limit_until_all_crates_are_published() {
        let crates = vec!["first".to_owned(), "second".to_owned(), "third".to_owned()];
        let published = std::cell::RefCell::new(Vec::<String>::new());
        let mut attempts = 0;
        let mut delays = Vec::new();
        publish_with_retries(
            &crates,
            |name| Ok(published.borrow().iter().any(|p| p == name)),
            |remaining| {
                if remaining.is_empty() {
                    return Ok(PublishOutcome::Complete);
                }
                attempts += 1;
                // Make progress only on the last allowed attempt of each cycle.
                if [6, 11, 16].contains(&attempts) {
                    published.borrow_mut().push(remaining[0].clone());
                }
                Ok(PublishOutcome::RateLimited)
            },
            |delay| delays.push(delay.as_secs()),
        )
        .unwrap();
        assert_eq!(attempts, 16);
        assert_eq!(*published.borrow(), crates);
        assert_eq!(delays, vec![60; 15]);
    }

    #[test]
    fn progress_does_not_allow_unlimited_stalled_retries() {
        let crates = vec!["first".to_owned(), "second".to_owned()];
        let published = std::cell::RefCell::new(Vec::<String>::new());
        let mut attempts = 0;
        let mut delays = Vec::new();
        let err = publish_with_retries(
            &crates,
            |name| Ok(published.borrow().iter().any(|p| p == name)),
            |remaining| {
                attempts += 1;
                if attempts == 6 {
                    published.borrow_mut().push(remaining[0].clone());
                }
                Ok(PublishOutcome::RateLimited)
            },
            |delay| delays.push(delay.as_secs()),
        )
        .unwrap_err();
        assert!(err.contains("without progress"));
        assert_eq!(attempts, 11);
        assert_eq!(delays, vec![60; 10]);
    }

    #[test]
    fn other_errors_stop_without_retrying() {
        for query_fails in [false, true] {
            let err = publish_with_retries(
                &["crate".into()],
                |_| {
                    if query_fails {
                        Err("query failed".into())
                    } else {
                        Ok(false)
                    }
                },
                |_| Err("publish failed".into()),
                |_| panic!("unexpected retry"),
            )
            .unwrap_err();
            assert_eq!(err, if query_fails { "query failed" } else { "publish failed" });
        }
    }

    #[cfg(unix)]
    #[test]
    fn cargo_exit_status_and_rate_limit_output_control_retries() {
        for (stderr, exit, dry_run, expected) in [
            (
                "error: status 429 Too Many Requests",
                101,
                false,
                Some(PublishOutcome::RateLimited),
            ),
            ("error: status 429 Too Many Requests", 101, true, None),
            ("error: status 403 Forbidden", 101, false, None),
            (
                "warning: 429 Too Many Requests",
                0,
                false,
                Some(PublishOutcome::Complete),
            ),
            ("Uploaded crate", 0, false, Some(PublishOutcome::Complete)),
        ] {
            let mut cmd = Command::new("sh");
            cmd.args([
                "-c",
                "printf '%s\\n' \"$1\" >&2; exit \"$2\"",
                "publish",
                stderr,
                &exit.to_string(),
            ]);
            let result = run_publish(&mut cmd, dry_run);
            match expected {
                Some(outcome) => assert_eq!(result.unwrap(), outcome),
                None => assert!(result.unwrap_err().contains("cargo publish failed")),
            }
        }
    }

    #[test]
    fn empty_selection_skips_cargo_in_both_modes() {
        for dry_run in [false, true] {
            // A nonexistent workspace proves we return before invoking Cargo.
            CratesRelease::new("1.2.3".into(), dry_run)
                .publish_crates(Path::new("/nonexistent-release-workspace"), &[])
                .unwrap();
        }
    }
}
