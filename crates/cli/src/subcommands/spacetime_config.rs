//! SpacetimeDB project configuration file handling.
//!
//! This module handles loading and saving `spacetime.toml` configuration files.
//! The config file is placed in the project root (same level as `spacetimedb/` directory).

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// The filename for configuration
pub const CONFIG_FILENAME: &str = "spacetime.toml";

/// Development mode configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DevConfig {
    /// The command to run the client development server.
    /// Example: "npm run dev", "pnpm dev", "cargo run"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_command: Option<String>,
}

/// Root configuration structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpacetimeConfig {
    /// Development mode configuration
    #[serde(default, skip_serializing_if = "DevConfig::is_empty")]
    pub dev: DevConfig,
}

impl DevConfig {
    fn is_empty(&self) -> bool {
        self.client_command.is_none()
    }
}

impl SpacetimeConfig {
    /// Create a new empty configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a configuration with a client command
    pub fn with_client_command(client_command: impl Into<String>) -> Self {
        Self {
            dev: DevConfig {
                client_command: Some(client_command.into()),
            },
        }
    }

    /// Load configuration from a directory.
    /// Returns `None` if no config file exists.
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Option<Self>> {
        let config_path = dir.join(CONFIG_FILENAME);

        if config_path.exists() {
            let content =
                fs::read_to_string(&config_path).with_context(|| format!("Failed to read {}", config_path.display()))?;
            let config: SpacetimeConfig =
                toml::from_str(&content).with_context(|| format!("Failed to parse {}", config_path.display()))?;
            return Ok(Some(config));
        }

        Ok(None)
    }

    /// Save configuration to `spacetime.toml` in the specified directory.
    pub fn save_to_dir(&self, dir: &Path) -> anyhow::Result<PathBuf> {
        let path = dir.join(CONFIG_FILENAME);
        let content = toml::to_string_pretty(self).context("Failed to serialize configuration")?;
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

/// Simple auto-detection for projects without `spacetime.toml`.
pub fn detect_client_command(project_dir: &Path) -> Option<String> {
    // JavaScript/TypeScript: package.json with "dev" script
    let package_json = project_dir.join("package.json");
    if package_json.exists() {
        if let Ok(content) = fs::read_to_string(&package_json) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if json.get("scripts").and_then(|s| s.get("dev")).is_some() {
                    return Some("npm run dev".to_string());
                }
            }
        }
    }

    // Rust: Cargo.toml
    if project_dir.join("Cargo.toml").exists() {
        return Some("cargo run".to_string());
    }

    // C#: .csproj file
    if let Ok(entries) = fs::read_dir(project_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|e| e == "csproj") {
                return Some("dotnet run".to_string());
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
        let config = SpacetimeConfig::with_client_command("npm run dev");

        config.save_to_dir(dir.path()).unwrap();

        let loaded = SpacetimeConfig::load_from_dir(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.dev.client_command, Some("npm run dev".to_string()));
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

        let config = SpacetimeConfig::with_client_command("npm run dev");
        config.save_to_dir(dir.path()).unwrap();

        assert!(SpacetimeConfig::exists_in_dir(dir.path()));
    }
}
