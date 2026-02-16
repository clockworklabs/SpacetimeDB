use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::generate::Language;
use crate::util::detect_module_language;

/// Configuration stored in spacetime.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacetimeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev: Option<DevConfig>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Development-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_dir: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Configuration stored in spacetime.local.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacetimeLocalConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Default for SpacetimeConfig {
    fn default() -> Self {
        Self {
            lang: None,
            database: None,
            env: None,
            dev: None,
            extra: HashMap::new(),
        }
    }
}

impl Default for SpacetimeLocalConfig {
    fn default() -> Self {
        Self {
            database: None,
            env: None,
            extra: HashMap::new(),
        }
    }
}

impl SpacetimeConfig {
    /// Load spacetime.json from the given directory, returning None if not found
    pub fn load(dir: &Path) -> Result<Option<Self>> {
        let config_path = dir.join("spacetime.json");
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                let config: SpacetimeConfig = serde_json::from_str(&content)
                    .with_context(|| format!("Failed to parse {}", config_path.display()))?;
                Ok(Some(config))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("Failed to read {}", config_path.display())),
        }
    }

    /// Save spacetime.json to the given directory
    pub fn save(&self, dir: &Path) -> Result<()> {
        let config_path = dir.join("spacetime.json");
        let content = serde_json::to_string_pretty(self).context("Failed to serialize spacetime.json")?;
        fs::write(&config_path, content).with_context(|| format!("Failed to write {}", config_path.display()))?;
        Ok(())
    }

    /// Create a new config with basic settings
    pub fn new_with_dev_run(run_command: String, lang: Option<String>) -> Self {
        Self {
            lang,
            database: None,
            env: Some("dev".to_string()),
            dev: Some(DevConfig {
                run: Some(run_command),
                module_path: None,
                out_dir: None,
                extra: HashMap::new(),
            }),
            extra: HashMap::new(),
        }
    }

    /// Detect project and look for spacetime config files
    pub fn find_project_root(start_dir: &Path) -> Option<PathBuf> {
        let mut current = start_dir.to_path_buf();
        loop {
            // Check for spacetime.json
            if current.join("spacetime.json").exists() {
                return Some(current);
            }

            // Check for spacetime.*.json
            if let Ok(entries) = fs::read_dir(&current) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with("spacetime.") && name_str.ends_with(".json") {
                        return Some(current);
                    }
                }
            }

            // Check for spacetimedb directory (existing detection)
            if current.join("spacetimedb").exists() {
                return Some(current);
            }

            if !current.pop() {
                break;
            }
        }
        None
    }

    /// Get database name, preferring local config over main config
    pub fn get_database<'a>(&'a self, local_config: Option<&'a SpacetimeLocalConfig>) -> Option<&'a str> {
        local_config
            .and_then(|l| l.database.as_deref())
            .or(self.database.as_deref())
    }

    /// Get environment, preferring local config over main config
    pub fn get_env<'a>(&'a self, local_config: Option<&'a SpacetimeLocalConfig>) -> Option<&'a str> {
        local_config.and_then(|l| l.env.as_deref()).or(self.env.as_deref())
    }
}

impl SpacetimeLocalConfig {
    /// Load spacetime.local.json from the given directory, returning None if not found
    pub fn load(dir: &Path) -> Result<Option<Self>> {
        let config_path = dir.join("spacetime.local.json");
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                let config: SpacetimeLocalConfig = serde_json::from_str(&content)
                    .with_context(|| format!("Failed to parse {}", config_path.display()))?;
                Ok(Some(config))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("Failed to read {}", config_path.display())),
        }
    }

    /// Save spacetime.local.json to the given directory
    pub fn save(&self, dir: &Path) -> Result<()> {
        let config_path = dir.join("spacetime.local.json");
        let content = serde_json::to_string_pretty(self).context("Failed to serialize spacetime.local.json")?;
        fs::write(&config_path, content).with_context(|| format!("Failed to write {}", config_path.display()))?;
        Ok(())
    }

    /// Create a new local config with database name
    pub fn new_with_database(database_name: String) -> Self {
        Self {
            database: Some(database_name),
            env: Some("dev".to_string()),
            extra: HashMap::new(),
        }
    }
}

/// Detect client language from project structure
pub fn detect_client_language(project_dir: &Path) -> Option<Language> {
    // Check for TypeScript/JavaScript files
    if project_dir.join("package.json").exists()
        || project_dir.join("tsconfig.json").exists()
        || project_dir.join("src").join("index.ts").exists()
        || project_dir.join("src").join("main.ts").exists()
    {
        return Some(Language::TypeScript);
    }

    // Check for Rust files
    if project_dir.join("Cargo.toml").exists() {
        return Some(Language::Rust);
    }

    // Check for C# files
    if let Ok(entries) = fs::read_dir(project_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().map(|ext| ext == "csproj").unwrap_or(false) {
                return Some(Language::Csharp);
            }
        }
    }

    None
}

/// Get default output directory for a language
pub fn get_default_out_dir(lang: &Language) -> &'static str {
    match lang {
        Language::TypeScript => "src/module_bindings",
        Language::Rust => "src/module_bindings",
        Language::Csharp => "module_bindings",
        Language::UnrealCpp => "Source/Generated",
    }
}

/// Try to discover language from project, preferring server module language for client bindings
pub fn discover_project_language(project_dir: &Path) -> Result<Option<Language>> {
    // First check if there's a spacetimedb directory and detect its language
    let spacetimedb_dir = project_dir.join("spacetimedb");
    if spacetimedb_dir.exists() {
        if let Ok(module_lang) = detect_module_language(&spacetimedb_dir) {
            return Ok(Some(match module_lang {
                crate::util::ModuleLanguage::Rust => Language::Rust,
                crate::util::ModuleLanguage::Csharp => Language::Csharp,
                crate::util::ModuleLanguage::Javascript => Language::TypeScript,
                crate::util::ModuleLanguage::Cpp => Language::Rust, // Default to Rust for C++
            }));
        }
    }

    // Fall back to detecting client language from project structure
    Ok(detect_client_language(project_dir))
}
