use crate::targets::{util, ReleaseTarget};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const CRATE_OWNERS: &[&str] = &["cloutiertyler", "jdetter", "bfops", "rekhoff", "spacetimedb-devops"];
const CRATES_IO_API: &str = "https://crates.io/api/v1";

pub struct CratesRelease {
    pub version: String,
    pub dry_run: bool,
}

impl CratesRelease {
    pub fn new(version: String, dry_run: bool) -> Self {
        Self { version, dry_run }
    }

    fn publish_crates(&self, workspace_root: &Path, crates: &[&String]) -> Result<(), String> {
        if crates.is_empty() {
            println!("All selected crates are already published; nothing to publish.");
            return Ok(());
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
        util::print_command(&cmd);
        let status = cmd
            .status()
            .map_err(|err| format!("Failed to execute cargo publish: {err}"))?;
        if !status.success() {
            return Err(format!("cargo publish failed: {status}"));
        }
        Ok(())
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
        // We must only publish unpublished crates, otherwise our publish invocation will fail.
        // This comes up in the context of re-running partially-completed releases if we hit an issue partway through.
        let mut unpublished = Vec::new();
        println!("Checking for already-published crates..");
        for crate_name in &crates {
            // Pace requests to respect crates.io's API guidance.
            std::thread::sleep(Duration::from_secs(1));
            if is_published(&client, CRATES_IO_API, crate_name, crates_io_version)? {
                println!("Crate {crate_name} already published; skipping.");
            } else {
                println!("Crate {crate_name} must be published.");
                unpublished.push(crate_name);
            }
        }
        self.publish_crates(&workspace_root, &unpublished)?;
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
    fn empty_selection_skips_cargo_in_both_modes() {
        for dry_run in [false, true] {
            // A nonexistent workspace proves we return before invoking Cargo.
            CratesRelease::new("1.2.3".into(), dry_run)
                .publish_crates(Path::new("/nonexistent-release-workspace"), &[])
                .unwrap();
        }
    }
}
