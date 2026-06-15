use crate::targets::util::print_command;
use crate::targets::ReleaseTarget;
use std::process::Command;

pub struct CppRelease {
    pub version: String,
    pub dry_run: bool,
}

impl CppRelease {
    pub fn new(version: String, dry_run: bool) -> Self {
        Self { version, dry_run }
    }
}

const CPP_REPO_URL: &str = "git@github.com:clockworklabs/spacetimedb-bindings-cpp.git";
const MIRROR_BRANCH: &str = "release/mirror/bindings-cpp";
const CPP_PREFIX: &str = "crates/bindings-cpp";

/// Configure the local git repo to use SSH instead of HTTPS for GitHub.
/// The checkout action installs a local URL rewrite that converts git@github.com: -> https://,
/// which overrides global config and breaks SSH-based pushes to external repos.
fn configure_git_ssh() {
    println!("Configuring git to use SSH...");

    // Remove the checkout action's SSH -> HTTPS rewrite
    let mut cmd = Command::new("git");
    cmd.args(["config", "--local", "--unset-all", "url.https://github.com/.insteadOf"]);
    print_command(&cmd);
    let _ = cmd.output();

    // Set HTTPS -> SSH so any https:// GitHub URL also uses SSH
    let mut cmd = Command::new("git");
    cmd.args([
        "config",
        "--local",
        "url.git@github.com:.insteadOf",
        "https://github.com/",
    ]);
    print_command(&cmd);
    let _ = cmd.output();
}

/// Fetch the release/mirror/bindings-cpp branch from origin.
/// Returns true if the branch exists on origin, false if it doesn't exist yet.
fn fetch_mirror_branch() -> Result<bool, String> {
    println!("\n=== Fetching C++ mirror branch ===");
    configure_git_ssh();
    println!("Fetching {} branch...", MIRROR_BRANCH);

    let mut cmd = Command::new("git");
    cmd.args(["fetch", "origin", MIRROR_BRANCH]);
    print_command(&cmd);
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute git fetch: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("couldn't find remote ref") {
            println!(
                "Branch {} doesn't exist yet - will create it on first push",
                MIRROR_BRANCH
            );
            return Ok(false);
        } else {
            return Err(format!("Failed to fetch {} branch: {}", MIRROR_BRANCH, stderr));
        }
    }

    println!("Successfully fetched {}", MIRROR_BRANCH);
    Ok(true)
}

/// Create a subtree split for the C++ bindings crate.
/// If `mirror_branch_exists` is true, passes `--onto origin/<mirror>` to speed up the split.
fn create_subtree_split(mirror_branch_exists: bool) -> Result<(), String> {
    println!("Creating subtree split for {}...", CPP_PREFIX);

    // Delete local branch if it exists
    let mut cmd = Command::new("git");
    cmd.args(["branch", "-D", MIRROR_BRANCH]);
    print_command(&cmd);
    let _ = cmd.output();

    let mut args = vec!["subtree", "split", "--prefix", CPP_PREFIX];
    let onto = format!("origin/{}", MIRROR_BRANCH);
    if mirror_branch_exists {
        args.extend(["--onto", &onto]);
    }
    args.extend(["-b", MIRROR_BRANCH]);

    let mut cmd = Command::new("git");
    cmd.args(&args);
    print_command(&cmd);
    let status = cmd
        .status()
        .map_err(|e| format!("Failed to execute git subtree split: {}", e))?;
    if !status.success() {
        return Err("Failed to create subtree split".to_string());
    }

    println!("Successfully created subtree split");
    Ok(())
}

/// Push to the C++ SDK repository as release/MAJOR.MINOR
fn push_to_cpp_repo_major_minor(version: &str, dry_run: bool) -> Result<(), String> {
    let parts: Vec<&str> = version.splitn(3, '.').collect();
    if parts.len() < 2 {
        return Err(format!(
            "Invalid version format (expected MAJOR.MINOR.PATCH): {}",
            version
        ));
    }
    let branch = format!("release/{}.{}", parts[0], parts[1]);
    println!("Pushing to C++ SDK repository ({})...", branch);

    if dry_run {
        println!("DRY RUN: Would execute:");
        println!("  git push -f {} {}:{}", CPP_REPO_URL, MIRROR_BRANCH, branch);
        return Ok(());
    }

    let mut cmd = Command::new("git");
    cmd.args(["push", "-f", CPP_REPO_URL, &format!("{}:{}", MIRROR_BRANCH, branch)]);
    print_command(&cmd);
    let status = cmd
        .status()
        .map_err(|e| format!("Failed to execute git push to C++ repo: {}", e))?;
    if !status.success() {
        return Err(format!("Failed to push to C++ SDK repository as {}", branch));
    }

    println!("Successfully pushed to C++ SDK repository as {}", branch);
    Ok(())
}

/// Push to the C++ SDK repository as release/latest
fn push_to_cpp_repo_latest(dry_run: bool) -> Result<(), String> {
    println!("Pushing to C++ SDK repository (release/latest)...");

    if dry_run {
        println!("DRY RUN: Would execute:");
        println!("  git push -f {} {}:release/latest", CPP_REPO_URL, MIRROR_BRANCH);
        return Ok(());
    }

    let mut cmd = Command::new("git");
    cmd.args(["push", "-f", CPP_REPO_URL, &format!("{}:release/latest", MIRROR_BRANCH)]);
    print_command(&cmd);
    let status = cmd
        .status()
        .map_err(|e| format!("Failed to execute git push to C++ repo: {}", e))?;
    if !status.success() {
        return Err("Failed to push to C++ SDK repository as release/latest".to_string());
    }

    println!("Successfully pushed to C++ SDK repository as release/latest");
    Ok(())
}

/// Push a version tag to the C++ SDK repository
fn push_to_cpp_repo_tag(version: &str, dry_run: bool) -> Result<(), String> {
    let tag_name = format!("v{}", version);
    println!("Pushing to C++ SDK repository (tag: {})...", tag_name);

    if dry_run {
        println!("DRY RUN: Would execute:");
        println!("  git push {} {}:refs/tags/{}", CPP_REPO_URL, MIRROR_BRANCH, tag_name);
        return Ok(());
    }

    let mut cmd = Command::new("git");
    cmd.args([
        "push",
        CPP_REPO_URL,
        &format!("{}:refs/tags/{}", MIRROR_BRANCH, tag_name),
    ]);
    print_command(&cmd);
    let status = cmd
        .status()
        .map_err(|e| format!("Failed to execute git push tag to C++ repo: {}", e))?;
    if !status.success() {
        return Err(format!("Failed to push tag {} to C++ SDK repository", tag_name));
    }

    println!("Successfully pushed tag {} to C++ SDK repository", tag_name);
    Ok(())
}

impl ReleaseTarget for CppRelease {
    fn release(&self) -> Result<(), String> {
        println!("=== Releasing C++ Bindings ===");
        println!("Version: {}", self.version);

        if self.dry_run {
            println!("\n*** DRY RUN MODE ***\n");
        }

        // Strip the leading 'v' from the version string
        let version = self.version.strip_prefix('v').unwrap_or(&self.version);

        let mirror_branch_exists = fetch_mirror_branch()?;
        create_subtree_split(mirror_branch_exists)?;
        push_to_cpp_repo_major_minor(version, self.dry_run)?;
        push_to_cpp_repo_latest(self.dry_run)?;
        push_to_cpp_repo_tag(version, self.dry_run)?;

        println!("\n=== C++ Bindings Release Complete ===");
        if !self.dry_run {
            let parts: Vec<&str> = version.splitn(3, '.').collect();
            println!("C++ bindings published to clockworklabs/spacetimedb-bindings-cpp:");
            if parts.len() >= 2 {
                println!("  - Branch: release/{}.{}", parts[0], parts[1]);
            }
            println!("  - Branch: release/latest");
            println!("  - Tag: v{}", version);
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "cpp"
    }
}
