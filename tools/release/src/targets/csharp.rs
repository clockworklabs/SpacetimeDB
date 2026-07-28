use crate::targets::util::print_command;
use crate::targets::ReleaseTarget;
use std::path::Path;
use std::process::Command;

pub struct CSharpRelease {
    pub version: String,
    pub dry_run: bool,
}

impl CSharpRelease {
    pub fn new(version: String, dry_run: bool) -> Self {
        Self { version, dry_run }
    }
}

/// Build the NuGet packages using dotnet pack (builds DLLs)
fn run_cargo_ci_dlls() -> Result<(), String> {
    println!("\n=== Running `cargo ci dlls` ===");

    let mut cmd = Command::new("cargo");
    cmd.args(["ci", "dlls"]);
    print_command(&cmd);
    let status = cmd
        .status()
        .map_err(|e| format!("Failed to run cargo ci dlls: {}", e))?;
    if !status.success() {
        return Err("Failed to run cargo ci dlls".to_string());
    }
    Ok(())
}

/// Push all NuGet packages
fn push_nuget_packages(version: &str, dry_run: bool) -> Result<(), String> {
    println!("\n=== Publishing NuGet Packages ===");

    // The NuGet package version comes from the csproj `<Version>`, which is not bumped
    // for a hotfix. So `dotnet pack` produces e.g. `SpacetimeDB.Runtime.2.7.0.nupkg`
    // even when releasing tag `v2.7.0-hotfix2`. Strip the `-hotfix<N>` suffix so we look
    // up the packages that were actually produced.
    let version = version.split_once("-hotfix").map_or(version, |(base, _)| base);

    let packages = vec![
        format!(
            "crates/bindings-csharp/BSATN.Runtime/bin/Release/SpacetimeDB.BSATN.Runtime.{}.nupkg",
            version
        ),
        format!(
            "crates/bindings-csharp/Runtime/bin/Release/SpacetimeDB.Runtime.{}.nupkg",
            version
        ),
        format!("sdks/csharp/bin~/Release/SpacetimeDB.ClientSDK.{}.nupkg", version),
        format!("sdks/csharp/bin~/Release/SpacetimeDB.ClientSDK.Godot.{}.nupkg", version),
    ];

    for package in &packages {
        if !dry_run && !Path::new(package).exists() {
            return Err(format!("Package not found: {}. Did dotnet pack succeed?", package));
        }
        println!("Pushing package: {}", package);
        push_nuget_package(package, dry_run)?;
    }

    println!("Successfully pushed all NuGet packages");
    Ok(())
}

/// Push a single NuGet package to the registry
fn push_nuget_package(package_path: &str, dry_run: bool) -> Result<(), String> {
    if dry_run {
        println!("DRY RUN: Would push package: {}", package_path);
        return Ok(());
    }

    // Get the NuGet API key from environment variable
    let api_key =
        std::env::var("NUGET_API_KEY").map_err(|_| "NUGET_API_KEY environment variable not set".to_string())?;
    let mut cmd = Command::new("nuget");
    cmd.args([
        "push",
        package_path,
        "-Source",
        "https://api.nuget.org/v3/index.json",
        "-ApiKey",
        &api_key,
        "-SkipDuplicate",
    ]);
    print_command(&cmd);
    let status = cmd
        .status()
        .map_err(|e| format!("Failed to execute nuget push: {}", e))?;
    if !status.success() {
        return Err("Failed to push NuGet package".to_string());
    }
    Ok(())
}

/// Update Unity project with new DLLs
fn commit_csharp_dlls_for_unity(version: &str) -> Result<(), String> {
    pub fn git_force_add(add_path: &str) -> Result<(), String> {
        let prefix = if add_path.contains("*") { ":(glob)" } else { "" };
        let add_path = format!("{}{}", prefix, add_path);
        let mut cmd = Command::new("git");
        cmd.args(["add", "-f", &add_path]);
        print_command(&cmd);
        let status = cmd
            .status()
            .map_err(|e| format!("Failed to force add git path: {}", e))?;
        if !status.success() {
            return Err("Failed to force add git path".to_string());
        }
        Ok(())
    }
    pub fn git_force_remove(path: &str) -> Result<(), String> {
        let prefix = if path.contains("*") { ":(glob)" } else { "" };
        let path = format!("{}{}", prefix, path);
        let mut cmd = Command::new("git");
        cmd.args(["rm", "-f", &path]);
        print_command(&cmd);
        let status = cmd
            .status()
            .map_err(|e| format!("Failed to force rm git path: {}", e))?;
        if !status.success() {
            return Err("Failed to force rm git path".to_string());
        }
        Ok(())
    }

    println!("\n=== Updating Unity Project with New DLLs ===");

    // Any meta files copied over by `cargo ci dlls` is a file we should keep.
    git_force_add("sdks/csharp/packages/**/*.meta")?;
    git_force_add("sdks/csharp/packages.meta")?;
    git_force_add(
        "sdks/csharp/packages/spacetimedb.bsatn.runtime/*/analyzers/dotnet/cs/SpacetimeDB.BSATN.Codegen.dll",
    )?;
    git_force_add("sdks/csharp/packages/spacetimedb.bsatn.runtime/*/lib/netstandard2.1/SpacetimeDB.BSATN.Runtime.dll")?;

    // Remove the gitignore file - Unity 6 will force-delete files that are listed in the gitignore
    git_force_remove("sdks/csharp/packages/.gitignore")?;

    // Remove the net8.0 files for now because we don't need them
    git_force_remove("sdks/csharp/packages/spacetimedb.bsatn.runtime/*/lib/net8.0.meta")?;
    git_force_remove("sdks/csharp/packages/spacetimedb.bsatn.runtime/*/lib/net8.0/SpacetimeDB.BSATN.Runtime.dll.meta")?;

    // Materialize the LICENSE.txt symlink
    let license_path = Path::new("sdks/csharp/LICENSE.txt");
    let target = std::fs::read_link(license_path).map_err(|e| format!("Failed to read LICENSE.txt symlink: {}", e))?;
    // resolve the path relative to the link's directory, since it basically has to be a relative path
    let target_path = license_path.parent().unwrap().join(&target);
    let contents = std::fs::read(&target_path).map_err(|e| format!("Failed to read LICENSE.txt target file: {}", e))?;
    std::fs::remove_file(license_path).map_err(|e| format!("Failed to remove LICENSE.txt symlink: {}", e))?;
    std::fs::write(license_path, contents).map_err(|e| format!("Failed to write LICENSE.txt file: {}", e))?;
    git_force_add("sdks/csharp/LICENSE.txt")?;

    // Print git status so we have an account of what has gone into this commit in the actions log
    let mut cmd = Command::new("git");
    cmd.args(["status"]);
    print_command(&cmd);
    let status = cmd.status().map_err(|e| format!("Failed to run git status: {}", e))?;
    if !status.success() {
        return Err("Failed to run git status".to_string());
    }

    let mut cmd = Command::new("git");
    cmd.args(["commit", "-m", &format!("Update Unity SDK to version v{}", version)]);
    print_command(&cmd);
    let status = cmd.status().map_err(|e| format!("Failed to run git commit: {}", e))?;
    if !status.success() {
        return Err("Failed to run git commit".to_string());
    }

    println!("\nSuccessfully updated Unity project");
    Ok(())
}

/// Fetch the release/mirror/csharp branch (or create it if it doesn't exist)
fn fetch_mirror_branch(dry_run: bool) -> Result<(), String> {
    println!("\n=== Publishing Unity SDK ===");

    // Ensure git uses SSH instead of HTTPS (in case global config doesn't apply to submodule)
    println!("Configuring git to use SSH...");

    // Remove the checkout action's HTTPS URL rewriting (SSH -> HTTPS conversion)
    let mut cmd = Command::new("git");
    cmd.args(["config", "--local", "--unset-all", "url.https://github.com/.insteadOf"]);
    print_command(&cmd);
    let _ = cmd.output();

    // Set up SSH preference (HTTPS -> SSH conversion)
    let mut cmd = Command::new("git");
    cmd.args([
        "config",
        "--local",
        "url.git@github.com:.insteadOf",
        "https://github.com/",
    ]);
    print_command(&cmd);
    let _ = cmd.output();

    // Debug: show URL rewrite config
    println!("  Git URL rewrite configuration:");
    let mut cmd = Command::new("git");
    cmd.args(["config", "--local", "--get-regexp", "url"]);
    print_command(&cmd);
    if let Ok(output) = cmd.output() {
        let config = String::from_utf8_lossy(&output.stdout);
        for line in config.lines() {
            println!("    {}", line);
        }
    }

    println!("Fetching release/mirror/csharp branch...");

    if dry_run {
        println!("DRY RUN: Would execute:");
        println!("  git fetch origin release/mirror/csharp");
        return Ok(());
    }

    // Try to fetch the branch - it's okay if it doesn't exist yet
    let mut cmd = Command::new("git");
    cmd.args(["fetch", "origin", "release/mirror/csharp"]);
    print_command(&cmd);
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute git fetch: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Check if the error is because the branch doesn't exist
        if stderr.contains("couldn't find remote ref") {
            println!("Branch release/mirror/csharp doesn't exist yet - will create it on first push");
            return Ok(());
        } else {
            return Err(format!("Failed to fetch release/mirror/csharp branch: {}", stderr));
        }
    }

    println!("Successfully fetched release/mirror/csharp");
    Ok(())
}

/// Create subtree split for the C# SDK
fn create_subtree_split(dry_run: bool) -> Result<(), String> {
    println!("Creating subtree split for sdks/csharp...");

    if dry_run {
        println!("DRY RUN: Would execute:");
        println!("  git subtree split --prefix=sdks/csharp -b release/mirror/csharp");
        return Ok(());
    }

    // Delete local branch if it exists
    let mut cmd = Command::new("git");
    cmd.args(["branch", "-D", "release/mirror/csharp"]);
    print_command(&cmd);
    let _ = cmd.output();

    // Build the git subtree split command
    let mut cmd = Command::new("git");
    cmd.args([
        "subtree",
        "split",
        "--prefix=sdks/csharp",
        "--onto",
        "origin/release/mirror/csharp",
        "-b",
        "release/mirror/csharp",
    ]);
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

/// Push to the Unity SDK repository as release/latest
fn push_to_unity_repo_latest(dry_run: bool) -> Result<(), String> {
    println!("Pushing to Unity SDK repository (release/latest)...");

    if dry_run {
        println!("DRY RUN: Would execute:");
        println!("  git push -f git@github.com:clockworklabs/com.clockworklabs.spacetimedbsdk.git release/mirror/csharp:release/latest");
        return Ok(());
    }

    let mut cmd = Command::new("git");
    cmd.args([
        "push",
        "-f",
        "git@github.com:clockworklabs/com.clockworklabs.spacetimedbsdk.git",
        "release/mirror/csharp:release/latest",
    ]);
    print_command(&cmd);
    let status = cmd
        .status()
        .map_err(|e| format!("Failed to execute git push to Unity repo: {}", e))?;

    if !status.success() {
        return Err("Failed to push to Unity SDK repository as release/latest".to_string());
    }

    println!("Successfully pushed to Unity SDK repository as release/latest");
    Ok(())
}

/// Push to the Unity SDK repository as a version tag
fn push_to_unity_repo_tag(version: &str, dry_run: bool) -> Result<(), String> {
    let tag_name = format!("v{}", version);
    println!("Pushing to Unity SDK repository (tag: {})...", tag_name);

    if dry_run {
        println!("DRY RUN: Would execute:");
        println!("  git push git@github.com:clockworklabs/com.clockworklabs.spacetimedbsdk.git release/mirror/csharp:refs/tags/{}", tag_name);
        return Ok(());
    }

    let mut cmd = Command::new("git");
    cmd.args([
        "push",
        "git@github.com:clockworklabs/com.clockworklabs.spacetimedbsdk.git",
        &format!("release/mirror/csharp:refs/tags/{}", tag_name),
    ]);
    print_command(&cmd);
    let status = cmd
        .status()
        .map_err(|e| format!("Failed to execute git push tag to Unity repo: {}", e))?;

    if !status.success() {
        return Err(format!("Failed to push tag {} to Unity SDK repository", tag_name));
    }

    println!("Successfully pushed tag {} to Unity SDK repository", tag_name);
    Ok(())
}

impl ReleaseTarget for CSharpRelease {
    fn release(&self) -> Result<(), String> {
        println!("=== Releasing C# SDK (NuGet + Unity) ===");
        println!("Version: {}", self.version);

        if self.dry_run {
            println!("\n*** DRY RUN MODE ***\n");
        }

        // Strip the leading 'v' from the version string
        let version = self.version.strip_prefix('v').unwrap_or(&self.version);

        run_cargo_ci_dlls()?;
        push_nuget_packages(version, self.dry_run)?;

        // TODO: It might be worth combining these 3 function calls (e.g. commit_unity_dlls)
        commit_csharp_dlls_for_unity(version)?;
        fetch_mirror_branch(self.dry_run)?;
        create_subtree_split(self.dry_run)?;

        // Note: We skip pushing to the public repo's origin since GitHub Actions
        // doesn't have permission. We push directly to the Unity SDK repo instead.
        push_to_unity_repo_latest(self.dry_run)?;
        push_to_unity_repo_tag(version, self.dry_run)?;

        println!("\n=== C# SDK Release Complete ===");
        if !self.dry_run {
            // NuGet package versions come from the csproj `<Version>` and are not bumped
            // for a hotfix, so report the base version here (see push_nuget_packages).
            let package_version = version.split_once("-hotfix").map_or(version, |(base, _)| base);
            println!("NuGet packages published:");
            println!("  - SpacetimeDB.BSATN.Runtime.{}", package_version);
            println!("  - SpacetimeDB.Runtime.{}", package_version);
            println!("  - SpacetimeDB.ClientSDK.{}", package_version);
            println!("  - SpacetimeDB.ClientSDK.Godot.{}", package_version);
            println!("\nUnity SDK published:");
            println!("  - Branch: release/latest");
            println!("  - Tag: v{}", version);
            println!("  - Repository: clockworklabs/com.clockworklabs.spacetimedbsdk");
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "csharp"
    }
}
