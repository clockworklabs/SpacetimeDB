use anyhow::Context;
use clap::{ArgMatches, Command, ValueEnum};
use json5;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::subcommands::generate::Language;

/// Errors that can occur when building or using CommandConfig
#[derive(Debug, Error)]
pub enum CommandConfigError {
    #[error("The option `--{arg_name}` is defined in Clap, but not in the config. If this is intentional and the option shouldn't be available in the config, you can exclude it with the `CommandConfigBuilder::exclude` function")]
    ClapArgNotDefined { arg_name: String },

    #[error("Key '{config_name}' references clap argument '{clap_name}' which doesn't exist in the Command. If the config key should be different than the clap argument, use from_clap()")]
    InvalidClapReference { config_name: String, clap_name: String },

    #[error("Key '{config_name}' has alias '{alias}' which doesn't exist in the Command")]
    InvalidAliasReference { config_name: String, alias: String },

    #[error("Excluded key '{key}' doesn't exist in the clap Command")]
    InvalidExclusion { key: String },

    #[error("Config key '{config_key}' is not supported in the config file. Available keys: {available_keys}")]
    UnsupportedConfigKey { config_key: String, available_keys: String },

    #[error("Required key '{key}' is missing from the config file")]
    MissingRequiredKey { key: String },

    #[error("Mismatch between definition and access of `{key}`. Could not downcast to {requested_type}, need to downcast to {expected_type}")]
    TypeMismatch {
        key: String,
        requested_type: String,
        expected_type: String,
    },

    #[error("Failed to convert config value for key '{key}' to type {target_type}")]
    ConversionError {
        key: String,
        target_type: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Project configuration loaded from spacetime.json.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SpacetimeConfig {
    /// Command to run after publishing and generating (used by `spacetime dev`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,

    /// List of generate configurations for creating client bindings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate: Option<Vec<HashMap<String, Value>>>,

    /// Configuration for publishing the database
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publish: Option<PublishConfig>,
}

/// Configuration for `spacetime publish` command.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct PublishConfig {
    /// Child databases
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<PublishConfig>>,

    /// Configuration fields
    #[serde(flatten)]
    pub additional_fields: HashMap<String, Value>,
}

impl PublishConfig {
    /// Iterate through all publish targets (self + children recursively).
    /// Returns an iterator that yields references to PublishConfig instances.
    pub fn iter_all_targets(&self) -> Box<dyn Iterator<Item = &PublishConfig> + '_> {
        Box::new(
            std::iter::once(self).chain(
                self.children
                    .iter()
                    .flat_map(|children| children.iter())
                    .flat_map(|child| child.iter_all_targets()),
            ),
        )
    }

    /// Count total number of targets (self + all descendants)
    pub fn count_targets(&self) -> usize {
        1 + self
            .children
            .as_ref()
            .map(|children| children.iter().map(|child| child.count_targets()).sum())
            .unwrap_or(0)
    }
}

/// A unified config that merges clap arguments with config file values.
/// Provides a `get_one::<T>(key)` interface similar to clap's ArgMatches.
/// CLI arguments take precedence over config file values.
#[derive(Debug)]
pub struct CommandConfig<'a> {
    /// Schema defining the contract between CLI and config
    schema: &'a CommandSchema,
    /// Config file values
    config_values: HashMap<String, Value>,
}

/// Schema that defines the contract between CLI arguments and config file keys.
/// Does not hold ArgMatches - methods take matches as a parameter instead.
#[derive(Debug)]
pub struct CommandSchema {
    /// Key definitions
    keys: Vec<Key>,
    /// Keys excluded from config file
    excluded_keys: HashSet<String>,
    /// Type information for validation (keyed by config name)
    type_map: HashMap<String, TypeId>,
    /// Map from config name to clap arg name (for from_clap mapping)
    config_to_clap: HashMap<String, String>,
    /// Map from config name to alias (for alias mapping)
    config_to_alias: HashMap<String, String>,
}

/// Builder for creating a CommandSchema with custom mappings and exclusions.
pub struct CommandSchemaBuilder {
    /// Keys defined for this command
    keys: Vec<Key>,
    /// Set of keys to exclude from being read from the config file
    excluded_keys: HashSet<String>,
}

impl CommandSchemaBuilder {
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            excluded_keys: HashSet::new(),
        }
    }

    /// Add a key definition to the builder.
    /// Example: `.key(Key::new::<String>("server"))`
    pub fn key(mut self, key: Key) -> Self {
        self.keys.push(key);
        self
    }

    /// Exclude a key from being read from the config file.
    /// This is useful for keys that should only come from CLI arguments.
    pub fn exclude(mut self, key: impl Into<String>) -> Self {
        self.excluded_keys.insert(key.into());
        self
    }

    /// Build a CommandSchema by validating against the clap Command.
    ///
    /// # Arguments
    /// * `command` - The clap Command to validate against
    pub fn build(self, command: &Command) -> Result<CommandSchema, CommandConfigError> {
        // Collect all clap argument names for validation
        let clap_arg_names: HashSet<String> = command
            .get_arguments()
            .map(|arg| arg.get_id().as_str().to_string())
            .collect();

        // Check that all the defined keys exist in clap
        for key in &self.keys {
            if !clap_arg_names.contains(key.clap_arg_name()) {
                return Err(CommandConfigError::InvalidClapReference {
                    config_name: key.config_name().to_string(),
                    clap_name: key.clap_arg_name().to_string(),
                });
            }

            // Validate alias if present
            if let Some(alias) = &key.clap_alias {
                if !clap_arg_names.contains(alias) {
                    return Err(CommandConfigError::InvalidAliasReference {
                        config_name: key.config_name().to_string(),
                        alias: alias.clone(),
                    });
                }
            }
        }

        // Validate exclusions reference valid clap arguments
        for excluded_key in &self.excluded_keys {
            if !clap_arg_names.contains(excluded_key) {
                return Err(CommandConfigError::InvalidExclusion {
                    key: excluded_key.clone(),
                });
            }
        }

        let mut type_map = HashMap::new();
        // A list of clap args that are referenced by the config keys
        let mut referenced_clap_args = HashSet::new();
        let mut config_to_clap_map = HashMap::new();
        let mut config_to_alias_map = HashMap::new();

        for key in &self.keys {
            let config_name = key.config_name().to_string();
            let clap_name = key.clap_arg_name().to_string();

            referenced_clap_args.insert(clap_name.clone());
            type_map.insert(config_name.clone(), key.type_id());

            // Track the mapping from config name to clap arg name (if using from_clap)
            if key.clap_name.is_some() {
                config_to_clap_map.insert(config_name.clone(), clap_name.clone());
            }

            // Register the alias if present
            if let Some(alias) = &key.clap_alias {
                referenced_clap_args.insert(alias.clone());
                config_to_alias_map.insert(config_name.clone(), alias.clone());
            }
        }

        // Check that all clap arguments are either referenced or excluded
        for arg in command.get_arguments() {
            let arg_name = arg.get_id().as_str();

            // Skip clap's built-in arguments
            if arg_name == "help" || arg_name == "version" {
                continue;
            }

            if !referenced_clap_args.contains(arg_name) && !self.excluded_keys.contains(arg_name) {
                return Err(CommandConfigError::ClapArgNotDefined {
                    arg_name: arg_name.to_string(),
                });
            }
        }

        Ok(CommandSchema {
            keys: self.keys,
            excluded_keys: self.excluded_keys,
            type_map,
            config_to_clap: config_to_clap_map,
            config_to_alias: config_to_alias_map,
        })
    }
}

impl Default for CommandSchemaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandSchema {
    /// Get a value from clap arguments only (not from config).
    /// Useful for filtering or checking if a value was provided via CLI.
    pub fn get_clap_arg<T: Clone + Send + Sync + 'static>(
        &self,
        matches: &ArgMatches,
        config_name: &str,
    ) -> Result<Option<T>, CommandConfigError> {
        let requested_type_id = TypeId::of::<T>();

        // Validate type if we have type information
        if let Some(&expected_type_id) = self.type_map.get(config_name) {
            if requested_type_id != expected_type_id {
                let expected_type_name = type_name_from_id(expected_type_id);
                let requested_type_name = std::any::type_name::<T>();

                return Err(CommandConfigError::TypeMismatch {
                    key: config_name.to_string(),
                    requested_type: requested_type_name.to_string(),
                    expected_type: expected_type_name.to_string(),
                });
            }
        }

        // Check clap with mapped name (if from_clap was used, use that name, otherwise use config name)
        let clap_name = self
            .config_to_clap
            .get(config_name)
            .map(|s| s.as_str())
            .unwrap_or(config_name);

        // Only return the value if it was actually provided by the user, not from defaults
        if let Some(source) = matches.value_source(clap_name) {
            if source == clap::parser::ValueSource::CommandLine {
                if let Some(value) = matches.get_one::<T>(clap_name) {
                    return Ok(Some(value.clone()));
                }
            }
        }

        // Try clap with the alias if it exists
        if let Some(alias) = self.config_to_alias.get(config_name) {
            if let Some(source) = matches.value_source(alias) {
                if source == clap::parser::ValueSource::CommandLine {
                    if let Some(value) = matches.get_one::<T>(alias) {
                        return Ok(Some(value.clone()));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Check if a value was provided via CLI (not from config).
    /// Only returns true if the user explicitly provided the value, not if it came from a default.
    pub fn is_from_cli(&self, matches: &ArgMatches, config_name: &str) -> bool {
        // Check clap with mapped name
        let clap_name = self
            .config_to_clap
            .get(config_name)
            .map(|s| s.as_str())
            .unwrap_or(config_name);

        // Use value_source to check if the value was actually provided by the user
        if let Some(source) = matches.value_source(clap_name) {
            if source == clap::parser::ValueSource::CommandLine {
                return true;
            }
        }

        // Check clap with alias
        if let Some(alias) = self.config_to_alias.get(config_name) {
            if let Some(source) = matches.value_source(alias) {
                if source == clap::parser::ValueSource::CommandLine {
                    return true;
                }
            }
        }

        false
    }

    /// Get all module-specific keys that were provided via CLI.
    pub fn module_specific_cli_args(&self, matches: &ArgMatches) -> Vec<&str> {
        self.keys
            .iter()
            .filter(|k| k.module_specific && self.is_from_cli(matches, k.config_name()))
            .map(|k| k.config_name())
            .collect()
    }
}

/// Configuration for a single key in the CommandConfig.
#[derive(Debug, Clone)]
pub struct Key {
    /// The key name in the config file (e.g., "module-path")
    config_name: String,
    /// The corresponding clap argument name (e.g., "project-path"), if different
    clap_name: Option<String>,
    /// Alias for a clap argument, useful for example if we have to deprecate a clap
    /// argument and still allow to use it in the CLI args, but not in the config file
    clap_alias: Option<String>,
    /// Whether this key is module-specific
    module_specific: bool,
    /// Whether this key is required in the config file
    required: bool,
    /// The expected TypeId for this key
    type_id: TypeId,
}

impl Key {
    /// Returns a new Key instance
    pub fn new<T: 'static>(name: impl Into<String>) -> Self {
        Self {
            config_name: name.into(),
            clap_name: None,
            clap_alias: None,
            module_specific: false,
            required: false,
            type_id: TypeId::of::<T>(),
        }
    }

    /// Map this config key to a different clap argument name. When fetching values
    /// the key that is defined should be used.
    /// Example: Key::new::<String>("module-path").from_clap("project-path")
    ///          - in this case the value for either project-path in clap or
    ///            for module-path in the config file will be fetched
    pub fn from_clap(mut self, clap_arg_name: impl Into<String>) -> Self {
        self.clap_name = Some(clap_arg_name.into());
        self
    }

    /// Add an alias for a clap argument name that also maps to this key.
    /// This is useful for backwards compatibility when renaming arguments.
    /// Example: Key::new::<String>("module-path").alias("project-path")
    ///
    /// This allows both --module-path and --project-path to map to the same config key.
    /// The value should then be accessed by using `module-path`
    ///
    /// The difference between from_clap and alias is that from_clap will work by mapping
    /// a single value from clap, whereas alias will check both of them in the CLI args
    pub fn alias(mut self, alias_name: impl Into<String>) -> Self {
        self.clap_alias = Some(alias_name.into());
        self
    }

    /// Mark this key as module-specific. For example, the `js-bin` config option makes sense
    /// only when applied to a single module. The `server` config option makes sense for
    /// multiple publish targets
    pub fn module_specific(mut self) -> Self {
        self.module_specific = true;
        self
    }

    /// Mark this key as required in the config file. If a config file is provided but
    /// this key is missing, an error will be returned.
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Get the clap argument name (either the mapped name or the config name)
    pub fn clap_arg_name(&self) -> &str {
        self.clap_name.as_deref().unwrap_or(&self.config_name)
    }

    /// Get the config name
    pub fn config_name(&self) -> &str {
        &self.config_name
    }

    /// Get the type_id
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Check if this key is required
    pub fn is_required(&self) -> bool {
        self.required
    }
}

impl<'a> CommandConfig<'a> {
    /// Create a new CommandConfig by validating config values against a schema.
    ///
    /// # Arguments
    /// * `schema` - The command schema that defines valid keys and types
    /// * `config_values` - Values from the config file
    ///
    /// # Errors
    /// Returns an error if any config keys are not defined in the schema.
    /// Note: Required key validation happens when get_one() is called, not during construction.
    pub fn new(schema: &'a CommandSchema, config_values: HashMap<String, Value>) -> Result<Self, CommandConfigError> {
        // Normalize keys from kebab-case to snake_case to match clap's Arg::new() convention
        let normalized_values: HashMap<String, Value> = config_values
            .into_iter()
            .map(|(k, v)| (k.replace('-', "_"), v))
            .collect();

        // Build set of valid config keys from schema
        let valid_config_keys: HashSet<String> = schema.keys.iter().map(|k| k.config_name().to_string()).collect();

        // Check that all keys in config file are defined in schema
        for config_key in normalized_values.keys() {
            if !valid_config_keys.contains(config_key) {
                return Err(CommandConfigError::UnsupportedConfigKey {
                    config_key: config_key.clone(),
                    available_keys: valid_config_keys
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                });
            }
        }

        Ok(CommandConfig {
            schema,
            config_values: normalized_values,
        })
    }

    /// Get a single value from the config as a specific type.
    /// First checks clap args (via schema), then falls back to config values.
    /// Validates that the requested type matches the schema definition.
    ///
    /// Returns:
    /// - Ok(Some(T)) if the value exists and can be converted
    /// - Ok(None) if the value doesn't exist in either clap or config
    /// - Err if the type doesn't match or conversion fails
    pub fn get_one<T: Clone + Send + Sync + 'static>(
        &self,
        matches: &ArgMatches,
        key: &str,
    ) -> Result<Option<T>, CommandConfigError> {
        // Try clap arguments first (CLI takes precedence) via schema
        let from_cli = self.schema.get_clap_arg::<T>(matches, key)?;
        if let Some(ref value) = from_cli {
            return Ok(Some(value.clone()));
        }

        // Fall back to config values using the config name
        if let Some(value) = self.config_values.get(key) {
            from_json_value::<T>(value)
                .map_err(|source| CommandConfigError::ConversionError {
                    key: key.to_string(),
                    target_type: std::any::type_name::<T>().to_string(),
                    source,
                })
                .map(Some)
        } else {
            Ok(None)
        }
    }

    /// Check if a key exists in either clap or config.
    pub fn contains(&self, matches: &ArgMatches, key: &str) -> bool {
        // Check if provided via CLI using schema
        if self.schema.is_from_cli(matches, key) {
            return true;
        }

        // Check config key
        self.config_values.contains_key(key)
    }

    /// Validate that all required keys are present in the config file.
    /// Note: This only checks config file keys. CLI required validation is handled by clap.
    pub fn validate(&self) -> Result<(), CommandConfigError> {
        for key in &self.schema.keys {
            if key.is_required() && !self.config_values.contains_key(key.config_name()) {
                return Err(CommandConfigError::MissingRequiredKey {
                    key: key.config_name().to_string(),
                });
            }
        }
        Ok(())
    }
}

/// Helper to get a human-readable type name from a TypeId
fn type_name_from_id(type_id: TypeId) -> &'static str {
    if type_id == TypeId::of::<String>() {
        "alloc::string::String"
    } else if type_id == TypeId::of::<PathBuf>() {
        "std::path::PathBuf"
    } else if type_id == TypeId::of::<bool>() {
        "bool"
    } else if type_id == TypeId::of::<i64>() {
        "i64"
    } else if type_id == TypeId::of::<u64>() {
        "u64"
    } else if type_id == TypeId::of::<f64>() {
        "f64"
    } else if type_id == TypeId::of::<Language>() {
        "spacetimedb_cli::subcommands::generate::Language"
    } else {
        "unknown"
    }
}

/// Helper to convert JSON values to Rust types (for config file values)
fn from_json_value<T: Clone + Send + Sync + 'static>(value: &Value) -> anyhow::Result<T> {
    let type_id = TypeId::of::<T>();

    let any: Box<dyn std::any::Any> = match type_id {
        t if t == TypeId::of::<String>() => Box::new(value.as_str().context("Expected string value")?.to_string()),
        t if t == TypeId::of::<PathBuf>() => Box::new(PathBuf::from(
            value.as_str().context("Expected string value for PathBuf")?,
        )),
        t if t == TypeId::of::<bool>() => Box::new(value.as_bool().context("Expected boolean value")?),
        t if t == TypeId::of::<i64>() => Box::new(value.as_i64().context("Expected i64 value")?),
        t if t == TypeId::of::<u64>() => Box::new(value.as_u64().context("Expected u64 value")?),
        t if t == TypeId::of::<f64>() => Box::new(value.as_f64().context("Expected f64 value")?),
        t if t == TypeId::of::<Language>() => {
            let s = value.as_str().context("Expected string value for Language")?;
            // Use ValueEnum's from_str method which handles aliases automatically
            let lang = Language::from_str(s, true).map_err(|_| anyhow::anyhow!("Invalid language: {}", s))?;
            Box::new(lang)
        }
        _ => anyhow::bail!("Unsupported type for conversion from JSON"),
    };

    // Now downcast to T
    any.downcast::<T>()
        .map(|boxed| *boxed)
        .map_err(|_| anyhow::anyhow!("Failed to downcast value"))
}

impl SpacetimeConfig {
    /// Find and load a spacetime.json file.
    ///
    /// Searches for spacetime.json starting from the current directory
    /// and walking up the directory tree until found or filesystem root is reached.
    ///
    /// Returns `Ok(Some((path, config)))` if found and successfully parsed.
    /// Returns `Ok(None)` if not found.
    /// Returns `Err` if found but failed to parse.
    pub fn find_and_load() -> anyhow::Result<Option<(PathBuf, Self)>> {
        Self::find_and_load_from(std::env::current_dir()?)
    }

    /// Find and load a spacetime.json file starting from a specific directory.
    ///
    /// Searches for spacetime.json starting from `start_dir`
    /// and walking up the directory tree until found or filesystem root is reached.
    pub fn find_and_load_from(start_dir: PathBuf) -> anyhow::Result<Option<(PathBuf, Self)>> {
        let mut current_dir = start_dir;
        loop {
            let config_path = current_dir.join("spacetime.json");
            if config_path.exists() {
                let config = Self::load(&config_path)
                    .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?;
                return Ok(Some((config_path, config)));
            }

            // Try to go up one directory
            if !current_dir.pop() {
                // Reached filesystem root
                break;
            }
        }
        Ok(None)
    }

    /// Load a spacetime.json file from a specific path.
    ///
    /// The file must exist and be valid JSON5 format (supports comments).
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config = json5::from_str(&content)
            .with_context(|| format!("Failed to parse config file as JSON: {}", path.display()))?;

        Ok(config)
    }

    /// Save the config to a file.
    ///
    /// The config will be serialized as pretty-printed JSON.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self).context("Failed to serialize config")?;

        std::fs::write(path, json).with_context(|| format!("Failed to write config file: {}", path.display()))?;

        Ok(())
    }

    /// Create a spacetime.json file in the current directory with the given config.
    pub fn create_in_current_dir(&self) -> anyhow::Result<PathBuf> {
        let config_path = std::env::current_dir()?.join("spacetime.json");
        self.save(&config_path)?;
        Ok(config_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_full_config() {
        let json = r#"{
            "run": "pnpm dev",
            "generate": [
                {
                    "out-dir": "./foobar",
                    "module-path": "region-module",
                    "language": "csharp"
                },
                {
                    "out-dir": "./global",
                    "module-path": "global-module",
                    "language": "csharp"
                }
            ],
            "publish": {
                "database": "bitcraft",
                "module-path": "spacetimedb",
                "server": "local",
                "children": [
                    {
                        "database": "region-1",
                        "module-path": "region-module"
                    },
                    {
                        "database": "region-2",
                        "module-path": "region-module"
                    }
                ]
            }
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();

        assert_eq!(config.run.as_deref(), Some("pnpm dev"));

        let generate = config.generate.as_ref().unwrap();
        assert_eq!(generate.len(), 2);
        assert_eq!(generate[0].get("out-dir").and_then(|v| v.as_str()), Some("./foobar"));
        assert_eq!(generate[0].get("language").and_then(|v| v.as_str()), Some("csharp"));

        let publish = config.publish.as_ref().unwrap();
        assert_eq!(
            publish.additional_fields.get("database").and_then(|v| v.as_str()),
            Some("bitcraft")
        );
        assert_eq!(
            publish.additional_fields.get("module-path").and_then(|v| v.as_str()),
            Some("spacetimedb")
        );

        let children = publish.children.as_ref().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(
            children[0].additional_fields.get("database").and_then(|v| v.as_str()),
            Some("region-1")
        );
        assert_eq!(
            children[1].additional_fields.get("database").and_then(|v| v.as_str()),
            Some("region-2")
        );
    }

    #[test]
    fn test_deserialize_with_comments() {
        let json = r#"{
            // This is a comment
            "run": "npm start",
            /* Multi-line comment */
            "generate": [
                {
                    "out-dir": "./src/bindings", // inline comment
                    "language": "typescript"
                }
            ]
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();
        assert_eq!(config.run.as_deref(), Some("npm start"));
    }

    #[test]
    fn test_minimal_config() {
        let json = r#"{}"#;
        let config: SpacetimeConfig = json5::from_str(json).unwrap();

        assert!(config.run.is_none());
        assert!(config.generate.is_none());
        assert!(config.publish.is_none());
    }

    #[test]
    fn test_project_config_builder() {
        use clap::{Arg, Command};

        // Create a simple clap command with some arguments
        let cmd = Command::new("test")
            .arg(Arg::new("out-dir").long("out-dir").value_name("DIR"))
            .arg(Arg::new("lang").long("lang").value_name("LANG"))
            .arg(Arg::new("server").long("server").value_name("SERVER"));

        let matches = cmd
            .clone()
            .get_matches_from(vec!["test", "--out-dir", "./bindings", "--lang", "typescript"]);

        // Build schema
        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("language").from_clap("lang"))
            .key(Key::new::<String>("out-dir"))
            .key(Key::new::<String>("server"))
            .build(&cmd)
            .unwrap();

        // Simulate config file values
        let mut config_values = HashMap::new();
        config_values.insert("language".to_string(), Value::String("rust".to_string()));
        config_values.insert("server".to_string(), Value::String("local".to_string()));

        // Create CommandConfig with schema
        let command_config = CommandConfig::new(&schema, config_values).unwrap();

        // CLI args should override config values
        assert_eq!(
            command_config.get_one::<String>(&matches, "out-dir").unwrap(),
            Some("./bindings".to_string())
        );
        assert_eq!(
            command_config.get_one::<String>(&matches, "language").unwrap(),
            Some("typescript".to_string())
        ); // CLI overrides (use config name, not clap name)
        assert_eq!(
            command_config.get_one::<String>(&matches, "server").unwrap(),
            Some("local".to_string())
        ); // from config
    }

    #[test]
    fn test_publish_config_extraction() {
        use clap::{Arg, Command};

        // Parse a PublishConfig from JSON
        let json = r#"{
            "database": "my-database",
            "server": "local",
            "module-path": "./my-module",
            "build-options": "--features extra",
            "break-clients": true,
            "anonymous": false
        }"#;

        let publish_config: PublishConfig = json5::from_str(json).unwrap();

        // Verify children field
        assert!(publish_config.children.is_none());

        // Verify all fields are in additional_fields
        assert_eq!(
            publish_config
                .additional_fields
                .get("database")
                .and_then(|v| v.as_str()),
            Some("my-database")
        );
        assert_eq!(
            publish_config.additional_fields.get("server").and_then(|v| v.as_str()),
            Some("local")
        );
        assert_eq!(
            publish_config
                .additional_fields
                .get("module-path")
                .and_then(|v| v.as_str()),
            Some("./my-module")
        );
        assert_eq!(
            publish_config
                .additional_fields
                .get("build-options")
                .and_then(|v| v.as_str()),
            Some("--features extra")
        );
        assert_eq!(
            publish_config
                .additional_fields
                .get("break-clients")
                .and_then(|v| v.as_bool()),
            Some(true)
        );

        // Now test merging with clap args
        let cmd = Command::new("test")
            .arg(Arg::new("database").long("database"))
            .arg(Arg::new("server").long("server"))
            .arg(Arg::new("module_path").long("module-path"))
            .arg(Arg::new("build_options").long("build-options"))
            .arg(Arg::new("break_clients").long("break-clients"))
            .arg(Arg::new("anon_identity").long("anonymous"));

        // CLI overrides the server
        let matches = cmd.clone().get_matches_from(vec!["test", "--server", "maincloud"]);

        // Build schema with snake_case keys
        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("database"))
            .key(Key::new::<String>("server"))
            .key(Key::new::<String>("module_path"))
            .key(Key::new::<String>("build_options"))
            .key(Key::new::<bool>("break_clients"))
            .key(Key::new::<bool>("anon_identity"))
            .build(&cmd)
            .unwrap();

        // Just pass the additional_fields directly - they will be normalized from kebab to snake_case
        let command_config = CommandConfig::new(&schema, publish_config.additional_fields).unwrap();

        // database comes from config
        assert_eq!(
            command_config.get_one::<String>(&matches, "database").unwrap(),
            Some("my-database".to_string())
        );
        // server comes from CLI (overrides config)
        assert_eq!(
            command_config.get_one::<String>(&matches, "server").unwrap(),
            Some("maincloud".to_string())
        );
        // module_path comes from config (kebab-case in JSON was normalized to snake_case)
        assert_eq!(
            command_config.get_one::<String>(&matches, "module_path").unwrap(),
            Some("./my-module".to_string())
        );
        // build_options comes from config
        assert_eq!(
            command_config.get_one::<String>(&matches, "build_options").unwrap(),
            Some("--features extra".to_string())
        );
    }

    #[test]
    fn test_type_mismatch_error() {
        use clap::{Arg, Command};

        let cmd = Command::new("test").arg(
            Arg::new("server")
                .long("server")
                .value_parser(clap::value_parser!(String)),
        );

        let matches = cmd.clone().get_matches_from(vec!["test", "--server", "local"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("server"))
            .build(&cmd)
            .unwrap();

        let command_config = CommandConfig::new(&schema, HashMap::new()).unwrap();

        // Trying to get as i64 when it's defined as String should error
        let result = command_config.get_one::<i64>(&matches, "server");
        assert!(matches!(
            result.unwrap_err(),
            CommandConfigError::TypeMismatch { key, requested_type, expected_type }
                if key == "server" && requested_type.contains("i64") && expected_type.contains("String")
        ));
    }

    #[test]
    fn test_schema_missing_key_definition_error() {
        use clap::{Arg, Command};

        // Define clap command with some arguments
        let cmd = Command::new("test")
            .arg(
                Arg::new("server")
                    .long("server")
                    .value_parser(clap::value_parser!(String)),
            )
            .arg(Arg::new("yes").long("yes").action(clap::ArgAction::SetTrue));

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        // Try to build schema but don't define all keys (missing "server" key)
        let result = CommandSchemaBuilder::new()
            .key(Key::new::<bool>("yes"))
            // Missing .key(Key::new::<String>("server"))
            .build(&cmd);

        // This should error because "server" is in clap but not defined in the builder
        // and not excluded
        assert!(matches!(
            result.unwrap_err(),
            CommandConfigError::ClapArgNotDefined { arg_name } if arg_name == "server"
        ));
    }

    #[test]
    fn test_key_with_clap_name_mapping() {
        use clap::{Arg, Command};

        // Clap uses "project-path" but config uses "module-path"
        let cmd = Command::new("test").arg(
            Arg::new("project-path")
                .long("project-path")
                .value_parser(clap::value_parser!(String)),
        );

        let matches = cmd
            .clone()
            .get_matches_from(vec!["test", "--project-path", "./my-project"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("module_path").from_clap("project-path"))
            .build(&cmd)
            .unwrap();

        // Config file uses "module-path" (kebab-case, will be normalized to module_path)
        let mut config_values = HashMap::new();
        config_values.insert("module-path".to_string(), Value::String("./config-project".to_string()));

        let command_config = CommandConfig::new(&schema, config_values).unwrap();

        // CLI should override config, accessed via config name "module_path" (snake_case)
        assert_eq!(
            command_config.get_one::<String>(&matches, "module_path").unwrap(),
            Some("./my-project".to_string())
        );
    }

    #[test]
    fn test_clap_argument_with_alias() {
        use clap::{Arg, Command};

        // Argument with both long name and alias
        let cmd = Command::new("test").arg(
            Arg::new("module-path")
                .long("module-path")
                .alias("project-path")
                .value_parser(clap::value_parser!(String)),
        );

        // Use the alias
        let matches = cmd
            .clone()
            .get_matches_from(vec!["test", "--project-path", "./my-project"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("module-path"))
            .build(&cmd)
            .unwrap();

        let command_config = CommandConfig::new(&schema, HashMap::new()).unwrap();

        // Should be accessible via the primary name
        assert_eq!(
            command_config.get_one::<String>(&matches, "module-path").unwrap(),
            Some("./my-project".to_string())
        );
    }

    #[test]
    fn test_optional_argument_not_provided() {
        use clap::{Arg, Command};

        let cmd = Command::new("test").arg(
            Arg::new("server")
                .long("server")
                .value_parser(clap::value_parser!(String)),
        );

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("server"))
            .build(&cmd)
            .unwrap();

        let command_config = CommandConfig::new(&schema, HashMap::new()).unwrap();

        // Should return Ok(None) when optional argument not provided
        assert_eq!(command_config.get_one::<String>(&matches, "server").unwrap(), None);
    }

    #[test]
    fn test_alias_support() {
        use clap::{Arg, Command};

        // Clap has both module-path and deprecated project-path
        let cmd = Command::new("test")
            .arg(
                Arg::new("module-path")
                    .long("module-path")
                    .value_parser(clap::value_parser!(String)),
            )
            .arg(
                Arg::new("project-path")
                    .long("project-path")
                    .value_parser(clap::value_parser!(String)),
            );

        // User uses the deprecated --project-path flag
        let matches = cmd
            .clone()
            .get_matches_from(vec!["test", "--project-path", "./deprecated"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("module-path").alias("project-path"))
            .build(&cmd)
            .unwrap();

        let command_config = CommandConfig::new(&schema, HashMap::new()).unwrap();

        // Should be able to get the value via the canonical name
        assert_eq!(
            command_config.get_one::<String>(&matches, "module-path").unwrap(),
            Some("./deprecated".to_string())
        );
    }

    #[test]
    fn test_alias_canonical_takes_precedence() {
        use clap::{Arg, Command};

        // Clap has both module-path and deprecated project-path
        let cmd = Command::new("test")
            .arg(
                Arg::new("module-path")
                    .long("module-path")
                    .value_parser(clap::value_parser!(String)),
            )
            .arg(
                Arg::new("project-path")
                    .long("project-path")
                    .value_parser(clap::value_parser!(String)),
            );

        // User provides BOTH flags (shouldn't happen but let's test precedence)
        let matches = cmd.clone().get_matches_from(vec![
            "test",
            "--module-path",
            "./canonical",
            "--project-path",
            "./deprecated",
        ]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("module-path").alias("project-path"))
            .build(&cmd)
            .unwrap();

        let command_config = CommandConfig::new(&schema, HashMap::new()).unwrap();

        // Canonical name should take precedence
        assert_eq!(
            command_config.get_one::<String>(&matches, "module-path").unwrap(),
            Some("./canonical".to_string())
        );
    }

    #[test]
    fn test_alias_with_config_fallback() {
        use clap::{Arg, Command};

        // Clap has both module_path and deprecated project-path as alias
        let cmd = Command::new("test")
            .arg(
                Arg::new("module_path")
                    .long("module-path")
                    .value_parser(clap::value_parser!(String)),
            )
            .arg(
                Arg::new("project-path")
                    .long("project-path")
                    .value_parser(clap::value_parser!(String)),
            );

        // User doesn't provide CLI args
        let matches = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("module_path").alias("project-path"))
            .build(&cmd)
            .unwrap();

        // Config has the value (kebab-case will be normalized)
        let mut config_values = HashMap::new();
        config_values.insert("module-path".to_string(), Value::String("./from-config".to_string()));

        let command_config = CommandConfig::new(&schema, config_values).unwrap();

        // Should fall back to config
        assert_eq!(
            command_config.get_one::<String>(&matches, "module_path").unwrap(),
            Some("./from-config".to_string())
        );
    }

    #[test]
    fn test_schema_invalid_from_clap_reference() {
        use clap::{Arg, Command};

        let cmd = Command::new("test").arg(
            Arg::new("server")
                .long("server")
                .value_parser(clap::value_parser!(String)),
        );

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        // Try to map to a non-existent clap arg
        let result = CommandSchemaBuilder::new()
            .key(Key::new::<String>("module-path").from_clap("non-existent"))
            .exclude("server") // Exclude the server arg we're not using
            .build(&cmd);

        assert!(matches!(
            result.unwrap_err(),
            CommandConfigError::InvalidClapReference { config_name, clap_name }
                if config_name == "module-path" && clap_name == "non-existent"
        ));
    }

    #[test]
    fn test_schema_invalid_alias_reference() {
        use clap::{Arg, Command};

        let cmd = Command::new("test").arg(
            Arg::new("module-path")
                .long("module-path")
                .value_parser(clap::value_parser!(String)),
        );

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        // Try to alias a non-existent clap arg
        let result = CommandSchemaBuilder::new()
            .key(Key::new::<String>("module-path").alias("non-existent-alias"))
            .build(&cmd);

        assert!(matches!(
            result.unwrap_err(),
            CommandConfigError::InvalidAliasReference { config_name, alias }
                if config_name == "module-path" && alias == "non-existent-alias"
        ));
    }

    #[test]
    fn test_undefined_config_key_error() {
        use clap::{Arg, Command};

        let cmd = Command::new("test").arg(
            Arg::new("server")
                .long("server")
                .value_parser(clap::value_parser!(String)),
        );

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("server"))
            .build(&cmd)
            .unwrap();

        // Config has a key that's not defined in CommandConfig
        let mut config_values = HashMap::new();
        config_values.insert("server".to_string(), Value::String("local".to_string()));
        config_values.insert("undefined-key".to_string(), Value::String("value".to_string()));

        let result = CommandConfig::new(&schema, config_values);

        // After normalization, "undefined-key" becomes "undefined_key"
        assert!(matches!(
            result.unwrap_err(),
            CommandConfigError::UnsupportedConfigKey { config_key, .. }
                if config_key == "undefined_key"
        ));
    }

    #[test]
    fn test_schema_from_clap_with_wrong_arg_name() {
        use clap::{Arg, Command};

        // Command has "lang" argument
        let cmd = Command::new("test").arg(Arg::new("lang").long("lang").value_parser(clap::value_parser!(String)));

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        // Try to create a key that references "language" via from_clap, but clap has "lang"
        let result = CommandSchemaBuilder::new()
            .key(Key::new::<String>("lang").from_clap("language"))
            .build(&cmd);

        // Should fail because "language" doesn't exist in the Command
        assert!(matches!(
            result.unwrap_err(),
            CommandConfigError::InvalidClapReference { config_name, clap_name }
                if config_name == "lang" && clap_name == "language"
        ));
    }

    #[test]
    fn test_excluded_key_in_config_should_error() {
        use clap::{Arg, Command};

        let cmd = Command::new("test")
            .arg(Arg::new("yes").long("yes").action(clap::ArgAction::SetTrue))
            .arg(Arg::new("server").long("server").value_name("SERVER"));

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("server"))
            .exclude("yes")
            .build(&cmd)
            .unwrap();

        // Config has yes, which is excluded
        let mut config_values = HashMap::new();
        config_values.insert("yes".to_string(), Value::Bool(true));
        config_values.insert("server".to_string(), Value::String("local".to_string()));

        let result = CommandConfig::new(&schema, config_values);

        // Should error because "yes" is excluded and shouldn't be in config
        assert!(matches!(
            result.unwrap_err(),
            CommandConfigError::UnsupportedConfigKey { config_key, .. }
                if config_key == "yes"
        ));
    }

    #[test]
    fn test_schema_get_clap_arg() {
        use clap::{Arg, Command};

        let cmd = Command::new("test")
            .arg(
                Arg::new("server")
                    .long("server")
                    .value_parser(clap::value_parser!(String)),
            )
            .arg(Arg::new("port").long("port").value_parser(clap::value_parser!(i64)));

        let matches = cmd
            .clone()
            .get_matches_from(vec!["test", "--server", "localhost", "--port", "8080"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("server"))
            .key(Key::new::<i64>("port"))
            .build(&cmd)
            .unwrap();

        // Should get values from CLI
        assert_eq!(
            schema.get_clap_arg::<String>(&matches, "server").unwrap(),
            Some("localhost".to_string())
        );
        assert_eq!(schema.get_clap_arg::<i64>(&matches, "port").unwrap(), Some(8080));
    }

    #[test]
    fn test_schema_is_from_cli() {
        use clap::{Arg, Command};

        let cmd = Command::new("test")
            .arg(
                Arg::new("server")
                    .long("server")
                    .value_parser(clap::value_parser!(String)),
            )
            .arg(Arg::new("port").long("port").value_parser(clap::value_parser!(i64)));

        let matches = cmd.clone().get_matches_from(vec!["test", "--server", "localhost"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("server"))
            .key(Key::new::<i64>("port"))
            .build(&cmd)
            .unwrap();

        // server was provided via CLI
        assert!(schema.is_from_cli(&matches, "server"));
        // port was not provided
        assert!(!schema.is_from_cli(&matches, "port"));
    }

    #[test]
    fn test_schema_module_specific_cli_args() {
        use clap::{Arg, Command};

        let cmd = Command::new("test")
            .arg(
                Arg::new("server")
                    .long("server")
                    .value_parser(clap::value_parser!(String)),
            )
            .arg(
                Arg::new("module-path")
                    .long("module-path")
                    .value_parser(clap::value_parser!(String)),
            )
            .arg(
                Arg::new("database")
                    .long("database")
                    .value_parser(clap::value_parser!(String)),
            );

        let matches = cmd
            .clone()
            .get_matches_from(vec!["test", "--module-path", "./module", "--server", "local"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("server"))
            .key(Key::new::<String>("module-path").module_specific())
            .key(Key::new::<String>("database"))
            .build(&cmd)
            .unwrap();

        let module_specific = schema.module_specific_cli_args(&matches);
        assert_eq!(module_specific.len(), 1);
        assert!(module_specific.contains(&"module-path"));
    }

    #[test]
    fn test_schema_get_clap_arg_with_from_clap() {
        use clap::{Arg, Command};

        let cmd = Command::new("test").arg(Arg::new("name").long("name").value_parser(clap::value_parser!(String)));

        let matches = cmd.clone().get_matches_from(vec!["test", "--name", "my-db"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("database").from_clap("name"))
            .build(&cmd)
            .unwrap();

        // Should get value using config name, which maps to clap arg "name"
        assert_eq!(
            schema.get_clap_arg::<String>(&matches, "database").unwrap(),
            Some("my-db".to_string())
        );
    }

    #[test]
    fn test_schema_get_clap_arg_with_alias() {
        use clap::{Arg, Command};

        let cmd = Command::new("test")
            .arg(
                Arg::new("module-path")
                    .long("module-path")
                    .value_parser(clap::value_parser!(String)),
            )
            .arg(
                Arg::new("project-path")
                    .long("project-path")
                    .value_parser(clap::value_parser!(String)),
            );

        let matches = cmd
            .clone()
            .get_matches_from(vec!["test", "--project-path", "./my-project"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("module-path").alias("project-path"))
            .build(&cmd)
            .unwrap();

        // Should get value from alias
        assert_eq!(
            schema.get_clap_arg::<String>(&matches, "module-path").unwrap(),
            Some("./my-project".to_string())
        );
    }

    #[test]
    fn test_schema_invalid_exclusion() {
        use clap::{Arg, Command};

        let cmd = Command::new("test").arg(
            Arg::new("server")
                .long("server")
                .value_parser(clap::value_parser!(String)),
        );

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        // Try to exclude a non-existent arg
        let result = CommandSchemaBuilder::new()
            .key(Key::new::<String>("server"))
            .exclude("non-existent")
            .build(&cmd);

        assert!(matches!(
            result.unwrap_err(),
            CommandConfigError::InvalidExclusion { key } if key == "non-existent"
        ));
    }

    #[test]
    fn test_config_value_type_conversion_error() {
        use clap::{Arg, Command};

        let cmd = Command::new("test").arg(Arg::new("port").long("port").value_parser(clap::value_parser!(i64)));

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<i64>("port"))
            .build(&cmd)
            .unwrap();

        // Config has a string value for port, but clap expects i64
        let mut config_values = HashMap::new();
        config_values.insert("port".to_string(), Value::String("not-a-number".to_string()));

        let command_config = CommandConfig::new(&schema, config_values).unwrap();

        // Should error when trying to convert invalid value
        let result = command_config.get_one::<i64>(&matches, "port");
        assert!(matches!(
            result.unwrap_err(),
            CommandConfigError::ConversionError { key, target_type, .. }
                if key == "port" && target_type.contains("i64")
        ));
    }

    #[test]
    fn test_validate_required_key_missing() {
        use clap::{Arg, Command};

        let cmd = Command::new("test")
            .arg(
                Arg::new("database")
                    .long("database")
                    .value_parser(clap::value_parser!(String)),
            )
            .arg(
                Arg::new("server")
                    .long("server")
                    .value_parser(clap::value_parser!(String)),
            );

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("database").required())
            .key(Key::new::<String>("server"))
            .build(&cmd)
            .unwrap();

        // Config is missing the required "database" key
        let config_values = HashMap::new();
        let command_config = CommandConfig::new(&schema, config_values).unwrap();

        // Should error on validation
        let result = command_config.validate();
        assert!(matches!(
            result.unwrap_err(),
            CommandConfigError::MissingRequiredKey { key }
                if key == "database"
        ));
    }

    #[test]
    fn test_validate_required_key_present() {
        use clap::{Arg, Command};

        let cmd = Command::new("test")
            .arg(
                Arg::new("database")
                    .long("database")
                    .value_parser(clap::value_parser!(String)),
            )
            .arg(
                Arg::new("server")
                    .long("server")
                    .value_parser(clap::value_parser!(String)),
            );

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("database").required())
            .key(Key::new::<String>("server"))
            .build(&cmd)
            .unwrap();

        // Config has the required database key
        let mut config_values = HashMap::new();
        config_values.insert("database".to_string(), Value::String("my-db".to_string()));

        let command_config = CommandConfig::new(&schema, config_values).unwrap();

        // Should succeed on validation
        assert!(command_config.validate().is_ok());
    }

    #[test]
    fn test_validate_no_required_keys() {
        use clap::{Arg, Command};

        let cmd = Command::new("test").arg(
            Arg::new("server")
                .long("server")
                .value_parser(clap::value_parser!(String)),
        );

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("server"))
            .build(&cmd)
            .unwrap();

        // No required keys, empty config should be fine
        let config_values = HashMap::new();
        let command_config = CommandConfig::new(&schema, config_values).unwrap();

        // Should succeed on validation
        assert!(command_config.validate().is_ok());
    }

    #[test]
    fn test_default_values_not_treated_as_cli() {
        use clap::{Arg, Command};
        use std::path::PathBuf;

        // Create a command with a default value
        let cmd = Command::new("test")
            .arg(
                Arg::new("project_path")
                    .long("project-path")
                    .value_parser(clap::value_parser!(PathBuf))
                    .default_value("."),
            )
            .arg(
                Arg::new("build_options")
                    .long("build-options")
                    .value_parser(clap::value_parser!(String))
                    .default_value(""),
            );

        // Get matches WITHOUT providing the arguments
        let matches = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<PathBuf>("project_path"))
            .key(Key::new::<String>("build_options"))
            .build(&cmd)
            .unwrap();

        // Config file has values
        let mut config_values = HashMap::new();
        config_values.insert("project_path".to_string(), Value::String("./my-module".to_string()));
        config_values.insert("build_options".to_string(), Value::String("--release".to_string()));

        let command_config = CommandConfig::new(&schema, config_values).unwrap();

        // Default values should NOT override config values
        assert_eq!(
            command_config.get_one::<PathBuf>(&matches, "project_path").unwrap(),
            Some(PathBuf::from("./my-module"))
        );
        assert_eq!(
            command_config.get_one::<String>(&matches, "build_options").unwrap(),
            Some("--release".to_string())
        );

        // is_from_cli should return false for default values
        assert!(!schema.is_from_cli(&matches, "project_path"));
        assert!(!schema.is_from_cli(&matches, "build_options"));
    }

    #[test]
    fn test_module_specific_only_checks_cli() {
        use clap::{Arg, Command};
        use std::path::PathBuf;

        let cmd = Command::new("test")
            .arg(
                Arg::new("project_path")
                    .long("project-path")
                    .value_parser(clap::value_parser!(PathBuf))
                    .default_value("."),
            )
            .arg(
                Arg::new("build_options")
                    .long("build-options")
                    .value_parser(clap::value_parser!(String))
                    .default_value(""),
            );

        // Test 1: No CLI args provided (only defaults)
        let matches_no_cli = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<PathBuf>("project_path").module_specific())
            .key(Key::new::<String>("build_options").module_specific())
            .build(&cmd)
            .unwrap();

        // module_specific_cli_args should be empty when only defaults are present
        let module_specific = schema.module_specific_cli_args(&matches_no_cli);
        assert!(module_specific.is_empty());

        // Test 2: CLI args actually provided
        let matches_with_cli = cmd.clone().get_matches_from(vec![
            "test",
            "--project-path",
            "./custom",
            "--build-options",
            "release-mode",
        ]);

        let module_specific = schema.module_specific_cli_args(&matches_with_cli);
        assert_eq!(module_specific.len(), 2);
        assert!(module_specific.contains(&"project_path"));
        assert!(module_specific.contains(&"build_options"));
    }

    #[test]
    fn test_kebab_case_normalization() {
        use clap::{Arg, Command};

        let cmd = Command::new("test").arg(
            Arg::new("build_options")
                .long("build-options")
                .value_parser(clap::value_parser!(String)),
        );

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new::<String>("build_options"))
            .build(&cmd)
            .unwrap();

        // Config file uses kebab-case
        let mut config_values = HashMap::new();
        config_values.insert("build-options".to_string(), Value::String("--release".to_string()));

        // The normalization in CommandConfig::new should convert build-options to build_options
        let command_config = CommandConfig::new(&schema, config_values).unwrap();

        // Should be able to access via snake_case key
        assert_eq!(
            command_config.get_one::<String>(&matches, "build_options").unwrap(),
            Some("--release".to_string())
        );
    }
}
