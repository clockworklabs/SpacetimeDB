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
    /// Parse a package manager from a string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "npm" => Some(PackageManager::Npm),
            "pnpm" => Some(PackageManager::Pnpm),
            "yarn" => Some(PackageManager::Yarn),
            "bun" => Some(PackageManager::Bun),
            _ => None,
        }
    }

    /// Get the command to run a dev script
    pub fn run_dev_command(&self) -> &'static str {
        match self {
            PackageManager::Npm => "npm run dev",
            PackageManager::Pnpm => "pnpm run dev",
            PackageManager::Yarn => "yarn dev",
            PackageManager::Bun => "bun run dev",
        }
    }

    /// Get the install command
    pub fn install_command(&self) -> &'static str {
        match self {
            PackageManager::Npm => "npm install",
            PackageManager::Pnpm => "pnpm install",
            PackageManager::Yarn => "yarn install",
            PackageManager::Bun => "bun install",
        }
    }
}

/// Root configuration structure for spacetime.json
///
/// Example:
/// ```json
/// {
///   // Command to run the client development server
///   "run": "pnpm dev"
/// }
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpacetimeConfig {
    /// The command to run the client development server.
    /// This is used by `spacetime dev` to start the client after publishing.
    /// Example: "npm run dev", "pnpm dev", "cargo run"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
}

impl SpacetimeConfig {
    /// Create a new empty configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a configuration with a run command
    pub fn with_run_command(run_command: impl Into<String>) -> Self {
        Self {
            run: Some(run_command.into()),
        }
    }

    /// Create a configuration with dev settings (for backward compatibility)
    pub fn with_dev_config(client_command: impl Into<String>, _package_manager: Option<PackageManager>) -> Self {
        Self {
            run: Some(client_command.into()),
        }
    }

    /// Load configuration from a directory.
    /// Returns `None` if no config file exists.
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Option<Self>> {
        let config_path = dir.join(CONFIG_FILENAME);

        if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read {}", config_path.display()))?;
            let config: SpacetimeConfig = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse {}", config_path.display()))?;
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

    /// Check if a config file exists in the directory
    pub fn exists_in_dir(dir: &Path) -> bool {
        dir.join(CONFIG_FILENAME).exists()
    }

    /// Get the path to the config file if it exists
    pub fn get_config_path(dir: &Path) -> Option<PathBuf> {
        let path = dir.join(CONFIG_FILENAME);
        if path.exists() {
            return Some(path);
        }
        None
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_save_and_load() {
        let dir = tempdir().unwrap();
        let config = SpacetimeConfig::with_run_command("npm run dev");

        config.save_to_dir(dir.path()).unwrap();

        let loaded = SpacetimeConfig::load_from_dir(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.run, Some("npm run dev".to_string()));
    }

    #[test]
    fn test_load_missing_config() {
        let dir = tempdir().unwrap();
        let loaded = SpacetimeConfig::load_from_dir(dir.path()).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_exists_in_dir() {
        let dir = tempdir().unwrap();
        assert!(!SpacetimeConfig::exists_in_dir(dir.path()));

        let config = SpacetimeConfig::with_run_command("npm run dev");
        config.save_to_dir(dir.path()).unwrap();

        assert!(SpacetimeConfig::exists_in_dir(dir.path()));
    }
}
