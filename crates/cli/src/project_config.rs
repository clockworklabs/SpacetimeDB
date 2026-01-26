use anyhow::Context;
use clap::{ArgMatches, Command};
use json5;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

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
    /// Database name/identity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,

    /// Child databases
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<PublishConfig>>,

    /// All other fields
    #[serde(flatten)]
    pub additional_fields: HashMap<String, Value>,
}

/// A unified config that merges clap arguments with config file values.
/// Provides a `get_one::<T>(key)` interface similar to clap's ArgMatches.
/// CLI arguments take precedence over config file values.
#[derive(Debug)]
pub struct CommandConfig<'a> {
    /// Clap argument matches
    matches: &'a ArgMatches,
    /// Config file values
    config_values: HashMap<String, Value>,
    /// Type information for validation
    arguments: HashMap<String, TypeId>,
    /// Map from clap arg name to its alias (if any)
    aliases: HashMap<String, String>,
}

/// Configuration for a single key in the CommandConfig.
#[derive(Debug, Clone)]
pub struct Key {
    /// The key name in the config file (e.g., "module-path")
    config_name: String,
    /// The corresponding clap argument name (e.g., "project-path"), if different
    clap_name: Option<String>,
    /// Alias for a clap argument, useful for example if we have to deprecate a clap
    /// argument, but still allow to use it in the CLI args, but not in the config file
    clap_alias: Option<String>,
    /// Whether this key is module-specific
    module_specific: bool,
    /// The expected TypeId for this key
    type_id: TypeId,
}

impl Key {
    pub fn new<T: 'static>(name: impl Into<String>) -> Self {
        Self {
            config_name: name.into(),
            clap_name: None,
            clap_alias: None,
            module_specific: false,
            type_id: TypeId::of::<T>(),
        }
    }

    /// Map this config key to a different clap argument name.
    /// Example: Key::new::<String>("module-path").from_clap("project-path")
    pub fn from_clap(mut self, clap_arg_name: impl Into<String>) -> Self {
        self.clap_name = Some(clap_arg_name.into());
        self
    }

    /// Add an alias for a clap argument name that also maps to this key.
    /// This is useful for backwards compatibility when renaming arguments.
    /// Example: Key::new::<String>("module-path").alias("project-path")
    ///
    /// This allows both --module-path and --project-path to map to the same config key.
    ///
    /// The difference between from_clap and alias is that from_clap will work by mapping
    /// a single value from clap, whereas alias will map both of them.
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
}

/// Builder for creating a CommandConfig with custom mappings and exclusions.
pub struct CommandConfigBuilder {
    /// Keys defined for this command
    keys: Vec<Key>,
    /// Set of keys to exclude from being read from the config file
    excluded_keys: HashSet<String>,
}

impl CommandConfigBuilder {
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

    /// Build a CommandConfig by merging clap arguments with config file values.
    ///
    /// # Arguments
    /// * `matches` - Parsed clap arguments (takes precedence)
    /// * `config_values` - Values from the config file (can be empty HashMap if no config)
    /// * `command` - The clap Command to validate against
    pub fn build<'a>(
        self,
        matches: &'a ArgMatches,
        config_values: HashMap<String, Value>,
        command: &Command,
    ) -> anyhow::Result<CommandConfig<'a>> {
        let mut processed_config_values = HashMap::new();
        let mut type_map = HashMap::new();

        let mut defined_clap_args = HashSet::new();
        let mut alias_map = HashMap::new();

        for key in &self.keys {
            defined_clap_args.insert(key.clap_arg_name().to_string());
            type_map.insert(key.clap_arg_name().to_string(), key.type_id());

            // Register the alias if present
            if let Some(alias) = &key.clap_alias {
                defined_clap_args.insert(alias.clone());
                type_map.insert(alias.clone(), key.type_id());
                // Map primary name -> alias for lookup in get_one()
                alias_map.insert(key.clap_arg_name().to_string(), alias.clone());
            }

            // If config values has a value for this key, map it to the clap name
            if let Some(value) = config_values.get(key.config_name()) {
                // Skip excluded keys
                if self.excluded_keys.contains(key.config_name()) {
                    continue;
                }
                processed_config_values.insert(key.clap_arg_name().to_string(), value.clone());
            }
        }

        // Validate: Check that all clap arguments are either defined or excluded
        // Skip clap's built-in arguments (help, version)
        for arg in command.get_arguments() {
            let arg_name = arg.get_id().as_str();

            // Skip clap's built-in arguments
            if arg_name == "help" || arg_name == "version" {
                continue;
            }

            if !defined_clap_args.contains(arg_name) && !self.excluded_keys.contains(arg_name) {
                anyhow::bail!(
                    "The option `--{}` is defined in Clap, but not in CommandConfig. \
                    If this is intentional and the option shouldn't be available in the config, \
                    you can exclude it with the `CommandConfigBuilder::exclude` function",
                    arg_name
                );
            }
        }

        // Validate: Check that all keys and exclusions reference valid clap arguments
        let clap_arg_names: HashSet<String> = command
            .get_arguments()
            .map(|arg| arg.get_id().as_str().to_string())
            .collect();

        for key in &self.keys {
            if !clap_arg_names.contains(key.clap_arg_name()) {
                anyhow::bail!(
                    "Key '{}' references clap argument '{}' which doesn't exist in the Command",
                    key.config_name(),
                    key.clap_arg_name()
                );
            }

            // Also validate alias if present
            if let Some(alias) = &key.clap_alias {
                if !clap_arg_names.contains(alias) {
                    anyhow::bail!(
                        "Key '{}' has alias '{}' which doesn't exist in the Command",
                        key.config_name(),
                        alias
                    );
                }
            }
        }

        for excluded_key in &self.excluded_keys {
            if !clap_arg_names.contains(excluded_key) {
                anyhow::bail!("Excluded key '{}' doesn't exist in the clap Command", excluded_key);
            }
        }

        // Also add any config values that weren't explicitly defined as keys
        for (key, value) in config_values {
            if !processed_config_values.contains_key(&key) && !self.excluded_keys.contains(&key) {
                processed_config_values.insert(key, value);
            }
        }

        Ok(CommandConfig {
            matches,
            config_values: processed_config_values,
            arguments: type_map,
            aliases: alias_map,
        })
    }
}

impl Default for CommandConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> CommandConfig<'a> {
    /// Get a single value from the config as a specific type.
    /// First checks clap args, then falls back to config values.
    /// Validates that the requested type matches the clap definition.
    ///
    /// Returns:
    /// - Ok(Some(T)) if the value exists and can be converted
    /// - Ok(None) if the value doesn't exist in either clap or config
    /// - Err if the type doesn't match or conversion fails
    pub fn get_one<T: Clone + Send + Sync + 'static>(&self, key: &str) -> anyhow::Result<Option<T>> {
        let requested_type_id = TypeId::of::<T>();

        // Validate type if we have type information
        if let Some(&expected_type_id) = self.arguments.get(key) {
            if requested_type_id != expected_type_id {
                let expected_type_name = type_name_from_id(expected_type_id);
                let requested_type_name = std::any::type_name::<T>();

                anyhow::bail!(
                    "Mismatch between definition and access of `{}`. Could not downcast to {}, need to downcast to {}",
                    key,
                    requested_type_name,
                    expected_type_name
                );
            }
        }

        // First try clap with the primary key name (takes precedence)
        if let Some(value) = self.matches.get_one::<T>(key) {
            return Ok(Some(value.clone()));
        }

        // Try clap with the alias if it exists
        if let Some(alias) = self.aliases.get(key) {
            if let Some(value) = self.matches.get_one::<T>(alias) {
                return Ok(Some(value.clone()));
            }
        }

        // Fall back to config values
        if let Some(value) = self.config_values.get(key) {
            from_json_value::<T>(value)
                .with_context(|| {
                    format!(
                        "Failed to convert config value for key '{}' to type {}",
                        key,
                        std::any::type_name::<T>()
                    )
                })
                .map(Some)
        } else {
            // Key doesn't exist - this is fine, most options are optional
            Ok(None)
        }
    }

    /// Check if a key exists in either clap or config.
    pub fn contains(&self, key: &str) -> bool {
        self.matches.contains_id(key)
            || self
                .aliases
                .get(key)
                .map_or(false, |alias| self.matches.contains_id(alias))
            || self.config_values.contains_key(key)
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
    } else {
        "unknown"
    }
}

/// Helper to convert JSON values to Rust types (for config file values)
fn from_json_value<T: Clone + Send + Sync + 'static>(value: &Value) -> anyhow::Result<T> {
    let type_id = TypeId::of::<T>();

    // We use a trick here: we create a Box<dyn Any> and then downcast it
    let any: Box<dyn std::any::Any> = if type_id == TypeId::of::<String>() {
        Box::new(value.as_str().context("Expected string value")?.to_string())
    } else if type_id == TypeId::of::<PathBuf>() {
        Box::new(PathBuf::from(
            value.as_str().context("Expected string value for PathBuf")?,
        ))
    } else if type_id == TypeId::of::<bool>() {
        Box::new(value.as_bool().context("Expected boolean value")?)
    } else if type_id == TypeId::of::<i64>() {
        Box::new(value.as_i64().context("Expected i64 value")?)
    } else if type_id == TypeId::of::<u64>() {
        Box::new(value.as_u64().context("Expected u64 value")?)
    } else if type_id == TypeId::of::<f64>() {
        Box::new(value.as_f64().context("Expected f64 value")?)
    } else {
        anyhow::bail!("Unsupported type for conversion from JSON")
    };

    // Now downcast to T
    any.downcast::<T>()
        .map(|boxed| *boxed)
        .map_err(|_| anyhow::anyhow!("Failed to downcast value"))
}

/// Trait for types that can be extracted from a JSON config value.
pub trait FromConfigValue: Sized {
    fn from_value(value: &Value) -> Option<Self>;
}

impl FromConfigValue for String {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_str().map(|s| s.to_string())
    }
}

impl FromConfigValue for PathBuf {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_str().map(PathBuf::from)
    }
}

impl FromConfigValue for bool {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_bool()
    }
}

impl FromConfigValue for i64 {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_i64()
    }
}

impl FromConfigValue for u64 {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_u64()
    }
}

impl FromConfigValue for f64 {
    fn from_value(value: &Value) -> Option<Self> {
        value.as_f64()
    }
}

impl<T: FromConfigValue> FromConfigValue for Vec<T> {
    fn from_value(value: &Value) -> Option<Self> {
        value
            .as_array()
            .and_then(|arr| arr.iter().map(|v| T::from_value(v)).collect())
    }
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
        assert_eq!(publish.database.as_deref(), Some("bitcraft"));
        assert_eq!(
            publish.additional_fields.get("module-path").and_then(|v| v.as_str()),
            Some("spacetimedb")
        );

        let children = publish.children.as_ref().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].database.as_deref(), Some("region-1"));
        assert_eq!(children[1].database.as_deref(), Some("region-2"));
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

        // Simulate config file values
        let mut config_values = HashMap::new();
        config_values.insert("language".to_string(), Value::String("rust".to_string()));
        config_values.insert("server".to_string(), Value::String("local".to_string()));

        // Build CommandConfig with key mapping (config uses "language", clap uses "lang")
        let command_config = CommandConfigBuilder::new()
            .key(Key::new::<String>("language").from_clap("lang"))
            .key(Key::new::<String>("out-dir"))
            .key(Key::new::<String>("server"))
            .build(&matches, config_values, &cmd)
            .unwrap();

        // CLI args should override config values
        assert_eq!(
            command_config.get_one::<String>("out-dir").unwrap(),
            Some("./bindings".to_string())
        );
        assert_eq!(
            command_config.get_one::<String>("lang").unwrap(),
            Some("typescript".to_string())
        ); // CLI overrides
        assert_eq!(
            command_config.get_one::<String>("server").unwrap(),
            Some("local".to_string())
        ); // from config
    }

    #[test]
    fn test_project_config_exclusions() {
        use clap::{Arg, Command};

        let cmd = Command::new("test")
            .arg(Arg::new("yes").long("yes").action(clap::ArgAction::SetTrue))
            .arg(Arg::new("server").long("server").value_name("SERVER"));

        let matches = cmd
            .clone()
            .get_matches_from(vec!["test", "--yes", "--server", "maincloud"]);

        // Config has yes, but we exclude it (yes should be CLI-only)
        let mut config_values = HashMap::new();
        config_values.insert("yes".to_string(), Value::Bool(false));
        config_values.insert("server".to_string(), Value::String("local".to_string()));

        let command_config = CommandConfigBuilder::new()
            .key(Key::new::<String>("server"))
            .exclude("yes")
            .build(&matches, config_values, &cmd)
            .unwrap();

        // yes should come from CLI only (config value was excluded)
        assert_eq!(command_config.get_one::<bool>("yes").unwrap(), Some(true));
        // server should come from CLI (overrides config)
        assert_eq!(
            command_config.get_one::<String>("server").unwrap(),
            Some("maincloud".to_string())
        );
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

        // Verify structured fields
        assert_eq!(publish_config.database.as_deref(), Some("my-database"));
        assert!(publish_config.children.is_none());

        // Verify additional_fields captured the other options
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
            .arg(Arg::new("module-path").long("module-path"))
            .arg(Arg::new("build-options").long("build-options"));

        // CLI overrides the server
        let matches = cmd.clone().get_matches_from(vec!["test", "--server", "maincloud"]);

        // Convert PublishConfig to HashMap for CommandConfig
        let mut config_values = publish_config.additional_fields.clone();
        if let Some(db) = publish_config.database.as_ref() {
            config_values.insert("database".to_string(), Value::String(db.clone()));
        }

        let command_config = CommandConfigBuilder::new()
            .key(Key::new::<String>("database"))
            .key(Key::new::<String>("server"))
            .key(Key::new::<String>("module-path"))
            .key(Key::new::<String>("build-options"))
            .build(&matches, config_values, &cmd)
            .unwrap();

        // database comes from config
        assert_eq!(
            command_config.get_one::<String>("database").unwrap(),
            Some("my-database".to_string())
        );
        // server comes from CLI (overrides config)
        assert_eq!(
            command_config.get_one::<String>("server").unwrap(),
            Some("maincloud".to_string())
        );
        // module-path comes from config
        assert_eq!(
            command_config.get_one::<String>("module-path").unwrap(),
            Some("./my-module".to_string())
        );
        // build-options comes from config
        assert_eq!(
            command_config.get_one::<String>("build-options").unwrap(),
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

        let command_config = CommandConfigBuilder::new()
            .key(Key::new::<String>("server"))
            .build(&matches, HashMap::new(), &cmd)
            .unwrap();

        // Trying to get as i64 when it's defined as String should error
        let result = command_config.get_one::<i64>("server");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Mismatch between definition and access"));
        assert!(err_msg.contains("server"));
        assert!(err_msg.contains("i64"));
        assert!(err_msg.contains("alloc::string::String"));
    }

    #[test]
    fn test_missing_key_definition_error() {
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

        // Try to build config but don't define all keys (missing "server" key)
        let result = CommandConfigBuilder::new()
            .key(Key::new::<bool>("yes"))
            // Missing .key(Key::new::<String>("server"))
            .build(&matches, HashMap::new(), &cmd);

        // This should error because "server" is in clap but not defined in the builder
        // and not excluded
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("server"));
        assert!(err_msg.contains("not in CommandConfig"));
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

        // Config file uses "module-path"
        let mut config_values = HashMap::new();
        config_values.insert("module-path".to_string(), Value::String("./config-project".to_string()));

        let command_config = CommandConfigBuilder::new()
            .key(Key::new::<String>("module-path").from_clap("project-path"))
            .build(&matches, config_values, &cmd)
            .unwrap();

        // CLI should override config, accessed via clap name "project-path"
        assert_eq!(
            command_config.get_one::<String>("project-path").unwrap(),
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

        let command_config = CommandConfigBuilder::new()
            .key(Key::new::<String>("module-path"))
            .build(&matches, HashMap::new(), &cmd)
            .unwrap();

        // Should be accessible via the primary name
        assert_eq!(
            command_config.get_one::<String>("module-path").unwrap(),
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

        let command_config = CommandConfigBuilder::new()
            .key(Key::new::<String>("server"))
            .build(&matches, HashMap::new(), &cmd)
            .unwrap();

        // Should return Ok(None) when optional argument not provided
        assert_eq!(command_config.get_one::<String>("server").unwrap(), None);
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

        let command_config = CommandConfigBuilder::new()
            .key(Key::new::<String>("module-path").alias("project-path"))
            .build(&matches, HashMap::new(), &cmd)
            .unwrap();

        // Should be able to get the value via the canonical name
        assert_eq!(
            command_config.get_one::<String>("module-path").unwrap(),
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

        let command_config = CommandConfigBuilder::new()
            .key(Key::new::<String>("module-path").alias("project-path"))
            .build(&matches, HashMap::new(), &cmd)
            .unwrap();

        // Canonical name should take precedence
        assert_eq!(
            command_config.get_one::<String>("module-path").unwrap(),
            Some("./canonical".to_string())
        );
    }

    #[test]
    fn test_alias_with_config_fallback() {
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

        // User doesn't provide CLI args
        let matches = cmd.clone().get_matches_from(vec!["test"]);

        // Config has the value
        let mut config_values = HashMap::new();
        config_values.insert("module-path".to_string(), Value::String("./from-config".to_string()));

        let command_config = CommandConfigBuilder::new()
            .key(Key::new::<String>("module-path").alias("project-path"))
            .build(&matches, config_values, &cmd)
            .unwrap();

        // Should fall back to config
        assert_eq!(
            command_config.get_one::<String>("module-path").unwrap(),
            Some("./from-config".to_string())
        );
    }

    #[test]
    fn test_invalid_from_clap_reference() {
        use clap::{Arg, Command};

        let cmd = Command::new("test").arg(
            Arg::new("server")
                .long("server")
                .value_parser(clap::value_parser!(String)),
        );

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        // Try to map to a non-existent clap arg
        let result = CommandConfigBuilder::new()
            .key(Key::new::<String>("module-path").from_clap("non-existent"))
            .exclude("server") // Exclude the server arg we're not using
            .build(&matches, HashMap::new(), &cmd);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("module-path") || err_msg.contains("non-existent"));
        assert!(err_msg.contains("doesn't exist in the Command"));
    }

    #[test]
    fn test_invalid_alias_reference() {
        use clap::{Arg, Command};

        let cmd = Command::new("test").arg(
            Arg::new("module-path")
                .long("module-path")
                .value_parser(clap::value_parser!(String)),
        );

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        // Try to alias a non-existent clap arg
        let result = CommandConfigBuilder::new()
            .key(Key::new::<String>("module-path").alias("non-existent-alias"))
            .build(&matches, HashMap::new(), &cmd);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("module-path"));
        assert!(err_msg.contains("non-existent-alias"));
        assert!(err_msg.contains("doesn't exist in the Command"));
    }

    #[test]
    fn test_config_value_type_conversion_error() {
        use clap::{Arg, Command};

        let cmd = Command::new("test").arg(Arg::new("port").long("port").value_parser(clap::value_parser!(i64)));

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        // Config has a string value for port, but clap expects i64
        let mut config_values = HashMap::new();
        config_values.insert("port".to_string(), Value::String("not-a-number".to_string()));

        let command_config = CommandConfigBuilder::new()
            .key(Key::new::<i64>("port"))
            .build(&matches, config_values, &cmd)
            .unwrap();

        // Should error when trying to convert invalid value
        let result = command_config.get_one::<i64>("port");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Failed to convert config value"));
        assert!(err_msg.contains("port"));
        assert!(err_msg.contains("i64"));
    }
}
