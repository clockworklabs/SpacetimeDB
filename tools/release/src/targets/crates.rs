use crate::targets::{util, ReleaseTarget};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

const CRATE_OWNERS: &[&str] = &["cloutiertyler", "jdetter", "bfops", "rekhoff", "spacetimedb-devops"];

pub struct CratesRelease {
    pub dry_run: bool,
}

impl CratesRelease {
    pub fn new(dry_run: bool) -> Self {
        Self { dry_run }
    }

    /// Publishes a single crate to crates.io
    fn publish_crate(&self, crate_name: &str, manifest_map: &HashMap<String, PathBuf>) -> Result<(), String> {
        println!("Publishing crate: {}", crate_name);

        let manifest_path = manifest_map
            .get(crate_name)
            .ok_or_else(|| format!("Crate '{}' not found in cargo metadata", crate_name))?;

        let crate_dir = manifest_path
            .parent()
            .ok_or_else(|| format!("Failed to get parent directory of {}", manifest_path.display()))?;

        let mut cmd_args = vec!["publish", "--allow-dirty"];
        if self.dry_run {
            cmd_args.push("--dry-run");
            cmd_args.push("--no-verify");
        }

        let mut cmd = Command::new("cargo");
        cmd.args(&cmd_args).current_dir(crate_dir);
        util::print_command(&cmd);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to execute cargo publish: {}", e))?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stderr.contains("already exists on crates.io index") {
            println!(
                "Crate {} already published on crates.io; skipping (treating as success).\n{}{}",
                crate_name, stdout, stderr
            );
            return Ok(());
        }

        Err(format!(
            "Failed to publish crate: {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            crate_name, stdout, stderr
        ))
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

impl ReleaseTarget for CratesRelease {
    fn release(&self) -> Result<(), String> {
        if self.dry_run {
            println!("cargo release crates --dry-run is currently not supported. See TODO in workflow file.");
            return Ok(());
        }

        println!("Finding crates to publish to crates.io...");

        // Get the workspace root and manifest map
        let workspace_root = crate::crates_resolver::find_workspace_root()
            .map_err(|e| format!("Failed to find workspace root: {}", e))?;
        let manifest_map = crate::crates_resolver::get_crate_manifest_map(&workspace_root)
            .map_err(|e| format!("Failed to get crate manifest map: {}", e))?;

        // TODO(bfops): we could pass manifest_map into get_crates_to_publish and then it wouldn't need to compute workspace_root or manifest_map again
        // alternatively, it could return richer data (e.g. including the crate path) and then we wouldn't need to compute manifest_map here
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
        for crate_name in &crates {
            self.publish_crate(crate_name, &manifest_map)?;
            self.add_crate_owners(crate_name)?;
        }

        println!("\nAll crates published successfully!");
        Ok(())
    }

    fn name(&self) -> &'static str {
        "crates"
    }
}
