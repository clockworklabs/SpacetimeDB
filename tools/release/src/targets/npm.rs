use crate::targets::{util, ReleaseTarget};
use std::path::Path;
use std::process::Command;

pub struct NpmRelease {
    pub version: String,
    pub dry_run: bool,
}

impl NpmRelease {
    pub fn new(version: String, dry_run: bool) -> Self {
        Self { version, dry_run }
    }

    /// Verify that pnpm is installed
    fn verify_pnpm(&self) -> Result<(), String> {
        println!("Verifying pnpm is installed...");

        let mut cmd = Command::new("pnpm");
        cmd.arg("--version");
        util::print_command(&cmd);
        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run pnpm command. Is pnpm installed? Error: {}", e))?;
        if !output.status.success() {
            return Err("pnpm is not available. Please install pnpm (npm install -g pnpm).".to_string());
        }

        let version = String::from_utf8_lossy(&output.stdout);
        println!("pnpm version: {}", version.trim());
        Ok(())
    }

    /// Publish the package using pnpm
    fn publish_package(&self) -> Result<(), String> {
        println!("\nPublishing TypeScript SDK to npm...");

        let sdk_dir = Path::new("sdks/typescript");

        println!("Running pnpm publish in {}...", sdk_dir.display());
        println!("Note: prepublishOnly script will build, test, and size up the package");

        // pnpm install first
        let mut cmd = Command::new("pnpm");
        cmd.args(["install"]);
        cmd.current_dir(sdk_dir);
        util::print_command(&cmd);
        let status = cmd
            .status()
            .map_err(|e| format!("Failed to execute pnpm install: {}", e))?;
        if !status.success() {
            return Err("Failed to run pnpm install".to_string());
        }

        let mut cmd = Command::new("pnpm");
        // The publish step here runs from a directory that doesn't have a clean git worktree
        // so we disable pnpm's default git cleanliness/branch checks otherwise this will fail.
        // We need --no-git-checks because otherwise the workflow will complain that we're not
        // on the main/master branch (we're in a detached HEAD state at this point).
        // ERR_PNPM_GIT_UNKNOWN_BRANCH The Git HEAD may not attached to any branch, but your "publish-branch" is set to "master|main".
        if self.dry_run {
            cmd.args(["publish", "--dry-run", "--no-git-checks"]);
        } else {
            cmd.args(["publish", "--no-git-checks"]);
        }
        cmd.current_dir(sdk_dir);
        util::print_command(&cmd);
        let status = cmd
            .status()
            .map_err(|e| format!("Failed to execute pnpm publish: {}", e))?;

        if !status.success() {
            return Err("Failed to publish package to npm".to_string());
        }

        println!(
            "Successfully{} published @clockworklabs/spacetimedb-sdk@{}",
            if self.dry_run { " dry-run" } else { "" },
            self.version
        );
        println!("See our package here: https://www.npmjs.com/package/spacetimedb");

        Ok(())
    }
}

impl ReleaseTarget for NpmRelease {
    fn release(&self) -> Result<(), String> {
        println!("=== Releasing TypeScript SDK to NPM ===");
        println!("Version: {}", self.version);
        println!("Package: @clockworklabs/spacetimedb-sdk");

        if self.dry_run {
            println!("\n*** DRY RUN MODE - Package will be built but NOT published ***\n");
        }

        self.verify_pnpm()?;
        self.publish_package()?;

        println!("\n=== NPM Release Complete ===");
        if !self.dry_run {
            println!("Package published: @clockworklabs/spacetimedb-sdk@{}", self.version);
            println!("Dist-tag 'latest' set to version {}", self.version);
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "npm"
    }
}
