//! SpacetimeDB project configuration file handling.
//!
//! This module handles loading and saving `spacetime.json` configuration files.
//! The config file is placed in the project root (same level as `spacetimedb/` directory).

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// The filename for configuration
pub const CONFIG_FILENAME: &str = "spacetime.json";

/// Supported package managers for JavaScript/TypeScript projects
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl fmt::Display for PackageManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PackageManager::Npm => "npm",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Yarn => "yarn",
            PackageManager::Bun => "bun",
        };
        write!(f, "{s}")
    }
}

impl PackageManager {
    /// Get the command to run a dev script
    pub fn run_dev_command(&self) -> &'static str {
        match self {
            PackageManager::Npm => "npm run dev",
            PackageManager::Pnpm => "pnpm run dev",
            PackageManager::Yarn => "yarn dev",
            PackageManager::Bun => "bun run dev",
        }
    }
}

/// Root configuration structure for spacetime.json
///
/// Example:
/// ```json
/// {
///   "dev_run": "pnpm dev"
/// }
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpacetimeConfig {
    /// The command to run the client development server.
    /// This is used by `spacetime dev` to start the client after publishing.
    /// Example: "npm run dev", "pnpm dev", "cargo run"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev_run: Option<String>,
}

impl SpacetimeConfig {
    /// Create a configuration with a dev_run command
    pub fn with_run_command(run_command: impl Into<String>) -> Self {
        Self {
            dev_run: Some(run_command.into()),
        }
    }

    /// Create a configuration for a specific client language.
    /// Determines the appropriate run command based on the language and package manager.
    pub fn for_client_lang(client_lang: &str, package_manager: Option<PackageManager>) -> Self {
        let run_command = match client_lang.to_lowercase().as_str() {
            "typescript" => package_manager.map(|pm| pm.run_dev_command()).unwrap_or("npm run dev"),
            "rust" => "cargo run",
            "csharp" | "c#" => "dotnet run",
            _ => "npm run dev", // default fallback
        };
        Self {
            dev_run: Some(run_command.to_string()),
        }
    }

    /// Load configuration from a directory.
    /// Returns `None` if no config file exists.
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Option<Self>> {
        let config_path = dir.join(CONFIG_FILENAME);

        if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read {}", config_path.display()))?;
            let config: SpacetimeConfig =
                serde_json::from_str(&content).with_context(|| format!("Failed to parse {}", config_path.display()))?;
            return Ok(Some(config));
        }

        Ok(None)
    }

    /// Save configuration to `spacetime.json` in the specified directory.
    pub fn save_to_dir(&self, dir: &Path) -> anyhow::Result<PathBuf> {
        let path = dir.join(CONFIG_FILENAME);
        let content = serde_json::to_string_pretty(self).context("Failed to serialize configuration")?;
        fs::write(&path, content).with_context(|| format!("Failed to write {}", path.display()))?;
        Ok(path)
    }
}

/// Set up a spacetime.json config for a project.
/// If `client_lang` is provided, creates a config for that language.
/// Otherwise, attempts to auto-detect from package.json.
/// Returns the path to the created config, or None if no config was created.
pub fn setup_for_project(
    project_path: &Path,
    client_lang: Option<&str>,
    package_manager: Option<PackageManager>,
) -> anyhow::Result<Option<PathBuf>> {
    if let Some(lang) = client_lang {
        let config = SpacetimeConfig::for_client_lang(lang, package_manager);
        return Ok(Some(config.save_to_dir(project_path)?));
    }

    if let Some((detected_cmd, _)) = detect_client_command(project_path) {
        return Ok(Some(
            SpacetimeConfig::with_run_command(&detected_cmd).save_to_dir(project_path)?,
        ));
    }

    Ok(None)
}

/// Detect the package manager from lock files in the project directory.
pub fn detect_package_manager(project_dir: &Path) -> Option<PackageManager> {
    // Check for lock files in order of preference
    if project_dir.join("pnpm-lock.yaml").exists() {
        return Some(PackageManager::Pnpm);
    }
    if project_dir.join("yarn.lock").exists() {
        return Some(PackageManager::Yarn);
    }
    if project_dir.join("bun.lockb").exists() || project_dir.join("bun.lock").exists() {
        return Some(PackageManager::Bun);
    }
    if project_dir.join("package-lock.json").exists() {
        return Some(PackageManager::Npm);
    }
    // Default to npm if package.json exists but no lock file
    if project_dir.join("package.json").exists() {
        return Some(PackageManager::Npm);
    }
    None
}

/// Simple auto-detection for projects without `spacetime.json`.
/// Returns the client command and optionally the detected package manager.
pub fn detect_client_command(project_dir: &Path) -> Option<(String, Option<PackageManager>)> {
    // JavaScript/TypeScript: package.json with "dev" script
    let package_json = project_dir.join("package.json");
    if package_json.exists() {
        if let Ok(content) = fs::read_to_string(&package_json) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                let has_dev = json.get("scripts").and_then(|s| s.get("dev")).is_some();
                if has_dev {
                    let pm = detect_package_manager(project_dir);
                    let cmd = pm.map(|p| p.run_dev_command()).unwrap_or("npm run dev");
                    return Some((cmd.to_string(), pm));
                }
            }
        }
    }

    // Rust: Cargo.toml
    if project_dir.join("Cargo.toml").exists() {
        return Some(("cargo run".to_string(), None));
    }

    // C#: .csproj file
    if let Ok(entries) = fs::read_dir(project_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|e| e == "csproj") {
                return Some(("dotnet run".to_string(), None));
            }
        }
    }

    None
}
