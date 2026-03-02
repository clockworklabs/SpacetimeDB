use anyhow::Context;
use clap::{ArgMatches, Command};
use path_clean::PathClean;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

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

    #[error("Required key '{key}' is missing from the config file or CLI")]
    MissingRequiredKey { key: String },

    #[error("Failed to convert config value for key '{key}' to type {target_type}")]
    ConversionError {
        key: String,
        target_type: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Project configuration loaded from spacetime.json.
///
/// The root object IS a database entity. `generate` is per-database
/// (inherited by children), and `dev` is root-only.
///
/// Example (simple):
/// ```json
/// {
///   "database": "my-database",
///   "server": "local",
///   "module-path": "./server",
///   "dev": { "run": "pnpm dev" },
///   "generate": [
///     { "language": "typescript", "out-dir": "./src/module_bindings" }
///   ]
/// }
/// ```
///
/// Example (multi-database):
/// ```json
/// {
///   "server": "local",
///   "module-path": "./server",
///   "children": [
///     { "database": "region-1" },
///     { "database": "region-2", "module-path": "./region-server" }
///   ]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SpacetimeConfig {
    /// Configuration for the dev command. Root-level only, not inherited.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev: Option<DevConfig>,

    /// Per-database generate entries. Inherited by children unless overridden.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate: Option<Vec<HashMap<String, Value>>>,

    /// Child database entities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<SpacetimeConfig>>,

    /// Name of the config file from which this target's `database` value was merged.
    #[serde(rename = "_source-config", skip_serializing)]
    pub source_config: Option<String>,

    /// All other entity-level fields (database, module-path, server, etc.)
    #[serde(flatten)]
    pub additional_fields: HashMap<String, Value>,
}

/// Configuration for `spacetime dev` command.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DevConfig {
    /// The command to run the client development server.
    /// This is used by `spacetime dev` to start the client after publishing.
    /// Example: "npm run dev", "pnpm dev", "cargo run"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
}

/// A fully resolved database target after inheritance.
/// Contains all fields needed for both publish and generate operations.
#[derive(Debug, Clone)]
pub struct FlatTarget {
    /// All entity-level fields (database, module-path, server, etc.)
    pub fields: HashMap<String, Value>,
    /// Name of the config file from which this target's `database` value was merged.
    pub source_config: Option<String>,
    /// Generate entries for this target (inherited or overridden)
    pub generate: Option<Vec<HashMap<String, Value>>>,
}

/// Result of loading config from one or more files.
pub struct LoadedConfig {
    pub config: SpacetimeConfig,
    pub config_dir: PathBuf,
    /// Which files contributed to this config
    pub loaded_files: Vec<PathBuf>,
    /// Whether a dev-specific file (spacetime.dev.json or spacetime.dev.local.json) was loaded
    pub has_dev_file: bool,
}

impl SpacetimeConfig {
    /// Collect all database targets with parent→child inheritance.
    /// Children inherit unset `additional_fields` and `generate` from their parent.
    /// `dev` and `children` are NOT propagated to child targets.
    /// Returns `Vec<FlatTarget>` with fully resolved fields.
    pub fn collect_all_targets_with_inheritance(&self) -> Vec<FlatTarget> {
        self.collect_targets_inner(None, None)
    }

    fn collect_targets_inner(
        &self,
        parent_fields: Option<&HashMap<String, Value>>,
        parent_generate: Option<&Vec<HashMap<String, Value>>>,
    ) -> Vec<FlatTarget> {
        // Build this node's fields by inheriting from parent
        let mut fields = self.additional_fields.clone();
        if let Some(parent) = parent_fields {
            for (key, value) in parent {
                if !fields.contains_key(key) {
                    fields.insert(key.clone(), value.clone());
                }
            }
        }

        // Generate: child's generate replaces parent's; if absent, inherit parent's
        let effective_generate = if self.generate.is_some() {
            self.generate.clone()
        } else {
            parent_generate.cloned()
        };

        let target = FlatTarget {
            fields: fields.clone(),
            source_config: self.source_config.clone(),
            generate: effective_generate.clone(),
        };

        let mut result = vec![target];

        if let Some(children) = &self.children {
            for child in children {
                let child_targets = child.collect_targets_inner(Some(&fields), effective_generate.as_ref());
                result.extend(child_targets);
            }
        }

        result
    }

    /// Iterate through all database targets (self + children recursively).
    /// Note: Does NOT apply parent→child inheritance. Use
    /// `collect_all_targets_with_inheritance()` for that.
    pub fn iter_all_targets(&self) -> Box<dyn Iterator<Item = &SpacetimeConfig> + '_> {
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
    /// CLI arguments
    matches: &'a ArgMatches,
}

/// Schema that defines the contract between CLI arguments and config file keys.
/// Does not hold ArgMatches - methods take matches as a parameter instead.
#[derive(Debug)]
pub struct CommandSchema {
    /// Key definitions
    keys: Vec<Key>,
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
    /// Example: `.key(Key::new("server"))`
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

        // A list of clap args that are referenced by the config keys
        let mut referenced_clap_args = HashSet::new();
        let mut config_to_clap_map = HashMap::new();
        let mut config_to_alias_map = HashMap::new();

        for key in &self.keys {
            let config_name = key.config_name().to_string();
            let clap_name = key.clap_arg_name().to_string();

            referenced_clap_args.insert(clap_name.clone());

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

    /// Get user-facing CLI flags (e.g. `--bin-path`) for all module-specific options
    /// that were explicitly provided via CLI.
    pub fn module_specific_cli_flags(&self, command: &Command, matches: &ArgMatches) -> Vec<String> {
        self.module_specific_cli_args(matches)
            .iter()
            .map(|arg| {
                let clap_name = self.clap_arg_name_for(arg);
                command
                    .get_arguments()
                    .find(|a| a.get_id().as_str() == clap_name)
                    .and_then(|a| a.get_long())
                    .map(|long| format!("--{long}"))
                    .unwrap_or_else(|| format!("--{}", clap_name.replace('_', "-")))
            })
            .collect()
    }

    /// Validate that module-specific CLI flags are not used when operating on multiple targets.
    pub fn validate_no_module_specific_cli_args_for_multiple_targets(
        &self,
        command: &Command,
        matches: &ArgMatches,
        target_count: usize,
        operation_context: &str,
        resolution_hint: &str,
    ) -> anyhow::Result<()> {
        if target_count <= 1 {
            return Ok(());
        }

        let display_args = self.module_specific_cli_flags(command, matches);
        if display_args.is_empty() {
            return Ok(());
        }

        anyhow::bail!(
            "Cannot use module-specific arguments ({}) when {}. {}",
            display_args.join(", "),
            operation_context,
            resolution_hint
        );
    }

    /// Get all generate-entry-specific keys that were provided via CLI.
    pub fn generate_entry_specific_cli_args(&self, matches: &ArgMatches) -> Vec<&str> {
        self.keys
            .iter()
            .filter(|k| k.generate_entry_specific && self.is_from_cli(matches, k.config_name()))
            .map(|k| k.config_name())
            .collect()
    }

    /// Get user-facing CLI flags for generate-entry-specific options provided via CLI.
    pub fn generate_entry_specific_cli_flags(&self, command: &Command, matches: &ArgMatches) -> Vec<String> {
        self.generate_entry_specific_cli_args(matches)
            .iter()
            .map(|arg| {
                let clap_name = self.clap_arg_name_for(arg);
                command
                    .get_arguments()
                    .find(|a| a.get_id().as_str() == clap_name)
                    .and_then(|a| a.get_long())
                    .map(|long| format!("--{long}"))
                    .unwrap_or_else(|| format!("--{}", clap_name.replace('_', "-")))
            })
            .collect()
    }

    /// Validate that generate-entry-specific CLI flags are not used when operating on multiple generate entries.
    pub fn validate_no_generate_entry_specific_cli_args(
        &self,
        command: &Command,
        matches: &ArgMatches,
        entry_count: usize,
    ) -> anyhow::Result<()> {
        if entry_count <= 1 {
            return Ok(());
        }

        let display_args = self.generate_entry_specific_cli_flags(command, matches);
        if display_args.is_empty() {
            return Ok(());
        }

        anyhow::bail!(
            "Cannot use generate-entry-specific arguments ({}) when generating for multiple entries. \
             Specify a database name to select a single target, or remove these arguments.",
            display_args.join(", "),
        );
    }

    /// Get the clap argument name for a config key.
    pub fn clap_arg_name_for<'a>(&'a self, config_name: &'a str) -> &'a str {
        self.config_to_clap
            .get(config_name)
            .map(|s| s.as_str())
            .unwrap_or(config_name)
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
    /// Whether this key is module-specific (per-database)
    module_specific: bool,
    /// Whether this key is generate-entry-specific (per-generate-entry within a database)
    generate_entry_specific: bool,
    /// Whether this key is required in the config file
    required: bool,
}

impl Key {
    /// Returns a new Key instance
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            config_name: name.into(),
            clap_name: None,
            clap_alias: None,
            module_specific: false,
            generate_entry_specific: false,
            required: false,
        }
    }

    /// Map this config key to a different clap argument name. When fetching values
    /// the key that is defined should be used.
    /// Example: Key::new("module-path").from_clap("project-path")
    ///          - in this case the value for either project-path in clap or
    ///            for module-path in the config file will be fetched
    pub fn from_clap(mut self, clap_arg_name: impl Into<String>) -> Self {
        self.clap_name = Some(clap_arg_name.into());
        self
    }

    /// Add an alias for a clap argument name that also maps to this key.
    /// This is useful for backwards compatibility when renaming arguments.
    /// Example: Key::new("module-path").alias("project-path")
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

    /// Mark this key as generate-entry-specific. These keys (like `language`, `out_dir`)
    /// only make sense when a single generate entry is targeted. If multiple generate
    /// entries exist and this key is provided via CLI, it's an error.
    pub fn generate_entry_specific(mut self) -> Self {
        self.generate_entry_specific = true;
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
    /// * `matches` - CLI arguments
    ///
    /// # Errors
    /// Returns an error if any config keys are not defined in the schema.
    /// Note: Required key validation happens when get_one() is called, not during construction.
    pub fn new(
        schema: &'a CommandSchema,
        config_values: HashMap<String, Value>,
        matches: &'a ArgMatches,
    ) -> Result<Self, CommandConfigError> {
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
            matches,
        })
    }

    /// Get a single value from the config as a specific type.
    /// First checks clap args (via schema), then falls back to config values.
    ///
    /// Returns:
    /// - Ok(Some(T)) if the value exists and can be converted
    /// - Ok(None) if the value doesn't exist in either clap or config
    /// - Err if conversion fails
    pub fn get_one<T: Clone + Send + Sync + serde::de::DeserializeOwned + 'static>(
        &self,
        key: &str,
    ) -> Result<Option<T>, CommandConfigError> {
        // Try clap arguments first (CLI takes precedence) via schema
        let from_cli = self.schema.get_clap_arg::<T>(self.matches, key)?;
        if let Some(ref value) = from_cli {
            return Ok(Some(value.clone()));
        }

        // Fall back to config values using the config name
        if let Some(value) = self.config_values.get(key) {
            serde_json::from_value::<T>(value.clone())
                .map_err(|e| CommandConfigError::ConversionError {
                    key: key.to_string(),
                    target_type: std::any::type_name::<T>().to_string(),
                    source: e.into(),
                })
                .map(Some)
        } else {
            Ok(None)
        }
    }

    /// Get a config value (from config file only, not merged with CLI).
    ///
    /// This is useful for filtering scenarios where you need to compare
    /// CLI values against config file values.
    pub fn get_config_value(&self, key: &str) -> Option<&Value> {
        self.config_values.get(key)
    }

    /// Get a path value and resolve it against `config_dir` if it came from config (not CLI).
    pub fn get_resolved_path(
        &self,
        key: &str,
        config_dir: Option<&Path>,
    ) -> Result<Option<PathBuf>, CommandConfigError> {
        let path = self.get_one::<PathBuf>(key)?;
        let from_cli = self.is_from_cli(key);
        Ok(path.map(|p| {
            let resolved = if p.is_absolute() || from_cli {
                p
            } else if let Some(base_dir) = config_dir {
                base_dir.join(p)
            } else {
                p
            };
            resolved.clean()
        }))
    }

    /// Returns true when this key was explicitly provided via CLI.
    pub fn is_from_cli(&self, key: &str) -> bool {
        self.schema.is_from_cli(self.matches, key)
    }

    /// Validate that all required keys are present in either config or CLI.
    pub fn validate(&self) -> Result<(), CommandConfigError> {
        for key in &self.schema.keys {
            if key.is_required()
                && !self.config_values.contains_key(key.config_name())
                && !self.schema.is_from_cli(self.matches, key.config_name())
            {
                return Err(CommandConfigError::MissingRequiredKey {
                    key: key.config_name().to_string(),
                });
            }
        }
        Ok(())
    }
}

impl SpacetimeConfig {
    /// Find and load a spacetime.json file (convenience wrapper for no env).
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
        Ok(find_and_load_with_env_from(None, start_dir)?.map(|loaded| {
            let config_path = loaded.config_dir.join(CONFIG_FILENAME);
            (config_path, loaded.config)
        }))
    }

    /// Load a spacetime.json file from a specific path.
    ///
    /// The file must exist and be valid JSON5 format (supports comments).
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Self = json5::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse config file {}: {}", path.display(), e))?;

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

    /// Create a configuration with a run command for dev
    pub fn with_run_command(run_command: impl Into<String>) -> Self {
        Self {
            dev: Some(DevConfig {
                run: Some(run_command.into()),
            }),
            ..Default::default()
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
            dev: Some(DevConfig {
                run: Some(run_command.to_string()),
            }),
            ..Default::default()
        }
    }

    /// Load configuration from a directory.
    /// Returns `None` if no config file exists.
    pub fn load_from_dir(dir: &Path) -> anyhow::Result<Option<Self>> {
        let config_path = dir.join(CONFIG_FILENAME);
        if config_path.exists() {
            Self::load(&config_path).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Save configuration to `spacetime.json` in the specified directory.
    pub fn save_to_dir(&self, dir: &Path) -> anyhow::Result<PathBuf> {
        let path = dir.join(CONFIG_FILENAME);
        self.save(&path)?;
        Ok(path)
    }
}

/// Find the config directory by walking up from start_dir looking for spacetime.json.
fn find_config_dir(start_dir: PathBuf) -> Option<PathBuf> {
    let mut current_dir = start_dir;
    loop {
        let config_path = current_dir.join("spacetime.json");
        if config_path.exists() {
            return Some(current_dir);
        }
        if !current_dir.pop() {
            break;
        }
    }
    None
}

/// Load a JSON5 file as a serde_json::Value, or None if the file doesn't exist.
fn load_json_value(path: &Path) -> anyhow::Result<Option<serde_json::Value>> {
    if !path.exists() {
        return Ok(None);
    }
    let content =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read config file: {}", path.display()))?;

    // In one of the releases we mistakenly save _source-config field into the JSON file
    // Check if the field exists and remove it. We use text-based removal to preserve
    // comments and formatting since json5 crate doesn't support serialization.
    remove_source_config_from_text(path, &content);

    let value: serde_json::Value = json5::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file {}: {}", path.display(), e))?;
    Ok(Some(value))
}

const SOURCE_CONFIG_KEY: &str = "_source-config";

/// Remove _source-config field from JSON text using regex
/// This preserves comments and formatting in the file
fn remove_source_config_from_text(path: &Path, content: &str) {
    if !content.contains(SOURCE_CONFIG_KEY) {
        return;
    }

    use regex::Regex;

    // Match "_source-config": "value", or "_source-config": "value"
    // Handles trailing comma and various whitespace patterns
    let re = Regex::new(r#"(?m)^\s*"_source-config"\s*:\s*"[^"]*"\s*,?\s*$\n?"#).unwrap();
    let cleaned = re.replace_all(content, "");

    // Also remove trailing commas that might be left behind before closing braces
    let re_trailing = Regex::new(r#"(?m),(\s*[\]}])"#).unwrap();
    let cleaned_content = re_trailing.replace_all(&cleaned, "$1").to_string();

    // Validate that the cleaned content is still valid JSON5
    // If validation fails, don't save
    if json5::from_str::<serde_json::Value>(&cleaned_content).is_err() {
        return;
    }

    // Write the cleaned content back to the file (best effort, ignore errors)
    let _ = std::fs::write(path, &cleaned_content);
}

fn mark_source_config(value: &mut serde_json::Value, source_file_name: &str) {
    if let Some(obj) = value.as_object_mut() {
        if obj.contains_key("database") {
            obj.insert(
                SOURCE_CONFIG_KEY.to_string(),
                serde_json::Value::String(source_file_name.to_string()),
            );
        }
        if let Some(serde_json::Value::Array(children)) = obj.get_mut("children") {
            for child in children {
                mark_source_config(child, source_file_name);
            }
        }
    }
}

fn overlay_children_arrays(
    base_children: &mut [serde_json::Value],
    overlay_children: Vec<serde_json::Value>,
    source_file_name: &str,
) {
    let merge_len = std::cmp::min(base_children.len(), overlay_children.len());
    for (idx, overlay_child) in overlay_children.into_iter().enumerate().take(merge_len) {
        let base_child = &mut base_children[idx];
        match overlay_child {
            serde_json::Value::Object(_) if base_child.is_object() => {
                // Recursively apply overlay semantics to child objects.
                overlay_json(base_child, overlay_child, source_file_name);
            }
            other => {
                // For non-object child entries, replace value directly.
                *base_child = other;
            }
        }
    }
}

/// Overlay `overlay` values onto `base`.
/// Most keys use top-level replacement. `children` is merged recursively by index,
/// up to the lower of base/overlay lengths.
fn overlay_json(base: &mut serde_json::Value, mut overlay: serde_json::Value, source_file_name: &str) {
    mark_source_config(&mut overlay, source_file_name);
    if let (Some(base_obj), Some(overlay_obj)) = (base.as_object_mut(), overlay.as_object()) {
        for (key, value) in overlay_obj {
            let value_owned = value.clone();
            if key == "children" {
                match (base_obj.get_mut("children"), value_owned) {
                    (Some(serde_json::Value::Array(base_children)), serde_json::Value::Array(overlay_children)) => {
                        overlay_children_arrays(base_children, overlay_children, source_file_name);
                    }
                    (_, other) => {
                        base_obj.insert(key.clone(), other);
                    }
                }
            } else {
                base_obj.insert(key.clone(), value_owned);
            }
        }
    }
}

/// Find and load config with environment layering from the current directory.
///
/// Loading order (each overlays the previous via top-level key replacement):
/// 1. `spacetime.json` (required)
/// 2. `spacetime.local.json` (if exists)
/// 3. `spacetime.<env>.json` (if env specified and file exists)
/// 4. `spacetime.<env>.local.json` (if env specified and file exists)
pub fn find_and_load_with_env(env: Option<&str>) -> anyhow::Result<Option<LoadedConfig>> {
    find_and_load_with_env_from(env, std::env::current_dir()?)
}

/// Find and load config with environment layering starting from a specific directory.
pub fn find_and_load_with_env_from(env: Option<&str>, start_dir: PathBuf) -> anyhow::Result<Option<LoadedConfig>> {
    let config_dir = match find_config_dir(start_dir) {
        Some(dir) => dir,
        None => return Ok(None),
    };

    let base_path = config_dir.join("spacetime.json");
    let mut merged = load_json_value(&base_path)?
        .ok_or_else(|| anyhow::anyhow!("spacetime.json not found in {}", config_dir.display()))?;
    mark_source_config(&mut merged, "spacetime.json");

    let mut loaded_files = vec![base_path];
    let mut has_dev_file = false;

    // Overlay local file
    let local_path = config_dir.join("spacetime.local.json");
    if let Some(local_value) = load_json_value(&local_path)? {
        overlay_json(&mut merged, local_value, "spacetime.local.json");
        loaded_files.push(local_path);
    }

    // Overlay environment-specific file
    if let Some(env_name) = env {
        let env_path = config_dir.join(format!("spacetime.{env_name}.json"));
        if let Some(env_value) = load_json_value(&env_path)? {
            overlay_json(&mut merged, env_value, &format!("spacetime.{env_name}.json"));
            loaded_files.push(env_path);
            if env_name == "dev" {
                has_dev_file = true;
            }
        }
    }

    // Overlay environment-specific local file
    if let Some(env_name) = env {
        let env_local_path = config_dir.join(format!("spacetime.{env_name}.local.json"));
        if let Some(env_local_value) = load_json_value(&env_local_path)? {
            overlay_json(
                &mut merged,
                env_local_value,
                &format!("spacetime.{env_name}.local.json"),
            );
            loaded_files.push(env_local_path);
            if env_name == "dev" {
                has_dev_file = true;
            }
        }
    }

    let config: SpacetimeConfig = serde_json::from_value(merged).context("Failed to deserialize merged config")?;

    Ok(Some(LoadedConfig {
        config,
        config_dir,
        loaded_files,
        has_dev_file,
    }))
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
    if project_path.join(CONFIG_FILENAME).exists() {
        return Ok(None);
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Arg;

    #[test]
    fn test_deserialize_full_config() {
        let json = r#"{
            "dev": {
                "run": "pnpm dev"
            },
            "database": "bitcraft",
            "module-path": "spacetimedb",
            "server": "local",
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
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();

        assert_eq!(config.dev.as_ref().and_then(|d| d.run.as_deref()), Some("pnpm dev"));

        let generate = config.generate.as_ref().unwrap();
        assert_eq!(generate.len(), 2);
        assert_eq!(generate[0].get("out-dir").and_then(|v| v.as_str()), Some("./foobar"));
        assert_eq!(generate[0].get("language").and_then(|v| v.as_str()), Some("csharp"));

        assert_eq!(
            config.additional_fields.get("database").and_then(|v| v.as_str()),
            Some("bitcraft")
        );
        assert_eq!(
            config.additional_fields.get("module-path").and_then(|v| v.as_str()),
            Some("spacetimedb")
        );

        let children = config.children.as_ref().unwrap();
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
            "dev": {
                "run": "npm start"
            },
            /* Multi-line comment */
            "generate": [
                {
                    "out-dir": "./src/bindings", // inline comment
                    "language": "typescript"
                }
            ]
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();
        assert_eq!(config.dev.as_ref().and_then(|d| d.run.as_deref()), Some("npm start"));
    }

    #[test]
    fn test_minimal_config() {
        let json = r#"{}"#;
        let config: SpacetimeConfig = json5::from_str(json).unwrap();

        assert!(config.dev.is_none());
        assert!(config.generate.is_none());
        assert!(config.children.is_none());
        assert!(config.additional_fields.is_empty());
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
            .key(Key::new("language").from_clap("lang"))
            .key(Key::new("out-dir"))
            .key(Key::new("server"))
            .build(&cmd)
            .unwrap();

        // Simulate config file values
        let mut config_values = HashMap::new();
        config_values.insert("language".to_string(), Value::String("rust".to_string()));
        config_values.insert("server".to_string(), Value::String("local".to_string()));

        // Create CommandConfig with schema
        let command_config = CommandConfig::new(&schema, config_values, &matches).unwrap();

        // CLI args should override config values
        assert_eq!(
            command_config.get_one::<String>("out-dir").unwrap(),
            Some("./bindings".to_string())
        );
        assert_eq!(
            command_config.get_one::<String>("language").unwrap(),
            Some("typescript".to_string())
        ); // CLI overrides (use config name, not clap name)
        assert_eq!(
            command_config.get_one::<String>("server").unwrap(),
            Some("local".to_string())
        ); // from config
    }

    #[test]
    fn test_database_entity_config_extraction() {
        use clap::{Arg, Command};

        // Parse a database entity config from JSON (database-centric model)
        let json = r#"{
            "database": "my-database",
            "server": "local",
            "module-path": "./my-module",
            "build-options": "--features extra",
            "break-clients": true,
            "anonymous": false
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();

        // Verify children field
        assert!(config.children.is_none());

        // Verify all fields are in additional_fields
        assert_eq!(
            config.additional_fields.get("database").and_then(|v| v.as_str()),
            Some("my-database")
        );
        assert_eq!(
            config.additional_fields.get("server").and_then(|v| v.as_str()),
            Some("local")
        );
        assert_eq!(
            config.additional_fields.get("module-path").and_then(|v| v.as_str()),
            Some("./my-module")
        );
        assert_eq!(
            config.additional_fields.get("build-options").and_then(|v| v.as_str()),
            Some("--features extra")
        );
        assert_eq!(
            config.additional_fields.get("break-clients").and_then(|v| v.as_bool()),
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
            .key(Key::new("database"))
            .key(Key::new("server"))
            .key(Key::new("module_path"))
            .key(Key::new("build_options"))
            .key(Key::new("break_clients"))
            // Config uses "anonymous", clap uses "anon_identity"
            .key(Key::new("anonymous").from_clap("anon_identity"))
            .build(&cmd)
            .unwrap();

        // Just pass the additional_fields directly - they will be normalized from kebab to snake_case
        let command_config = CommandConfig::new(&schema, config.additional_fields, &matches).unwrap();

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
        // module_path comes from config (kebab-case in JSON was normalized to snake_case)
        assert_eq!(
            command_config.get_one::<String>("module_path").unwrap(),
            Some("./my-module".to_string())
        );
        // build_options comes from config
        assert_eq!(
            command_config.get_one::<String>("build_options").unwrap(),
            Some("--features extra".to_string())
        );
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

        // Try to build schema but don't define all keys (missing "server" key)
        let result = CommandSchemaBuilder::new()
            .key(Key::new("yes"))
            // Missing .key(Key::new("server"))
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
            .key(Key::new("module_path").from_clap("project-path"))
            .build(&cmd)
            .unwrap();

        // Config file uses "module-path" (kebab-case, will be normalized to module_path)
        let mut config_values = HashMap::new();
        config_values.insert("module-path".to_string(), Value::String("./config-project".to_string()));

        let command_config = CommandConfig::new(&schema, config_values, &matches).unwrap();

        // CLI should override config, accessed via config name "module_path" (snake_case)
        assert_eq!(
            command_config.get_one::<String>("module_path").unwrap(),
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
            .key(Key::new("module-path"))
            .build(&cmd)
            .unwrap();

        let command_config = CommandConfig::new(&schema, HashMap::new(), &matches).unwrap();

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

        let schema = CommandSchemaBuilder::new().key(Key::new("server")).build(&cmd).unwrap();

        let command_config = CommandConfig::new(&schema, HashMap::new(), &matches).unwrap();

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

        let schema = CommandSchemaBuilder::new()
            .key(Key::new("module-path").alias("project-path"))
            .build(&cmd)
            .unwrap();

        let command_config = CommandConfig::new(&schema, HashMap::new(), &matches).unwrap();

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

        let schema = CommandSchemaBuilder::new()
            .key(Key::new("module-path").alias("project-path"))
            .build(&cmd)
            .unwrap();

        let command_config = CommandConfig::new(&schema, HashMap::new(), &matches).unwrap();

        // Canonical name should take precedence
        assert_eq!(
            command_config.get_one::<String>("module-path").unwrap(),
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
            .key(Key::new("module_path").alias("project-path"))
            .build(&cmd)
            .unwrap();

        // Config has the value (kebab-case will be normalized)
        let mut config_values = HashMap::new();
        config_values.insert("module-path".to_string(), Value::String("./from-config".to_string()));

        let command_config = CommandConfig::new(&schema, config_values, &matches).unwrap();

        // Should fall back to config
        assert_eq!(
            command_config.get_one::<String>("module_path").unwrap(),
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

        // Try to map to a non-existent clap arg
        let result = CommandSchemaBuilder::new()
            .key(Key::new("module-path").from_clap("non-existent"))
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

        // Try to alias a non-existent clap arg
        let result = CommandSchemaBuilder::new()
            .key(Key::new("module-path").alias("non-existent-alias"))
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

        let schema = CommandSchemaBuilder::new().key(Key::new("server")).build(&cmd).unwrap();

        // Config has a key that's not defined in CommandConfig
        let mut config_values = HashMap::new();
        config_values.insert("server".to_string(), Value::String("local".to_string()));
        config_values.insert("undefined-key".to_string(), Value::String("value".to_string()));

        let result = CommandConfig::new(&schema, config_values, &matches);

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

        // Try to create a key that references "language" via from_clap, but clap has "lang"
        let result = CommandSchemaBuilder::new()
            .key(Key::new("lang").from_clap("language"))
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
            .key(Key::new("server"))
            .exclude("yes")
            .build(&cmd)
            .unwrap();

        // Config has yes, which is excluded
        let mut config_values = HashMap::new();
        config_values.insert("yes".to_string(), Value::Bool(true));
        config_values.insert("server".to_string(), Value::String("local".to_string()));

        let result = CommandConfig::new(&schema, config_values, &matches);

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
            .key(Key::new("server"))
            .key(Key::new("port"))
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
            .key(Key::new("server"))
            .key(Key::new("port"))
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
            .key(Key::new("server"))
            .key(Key::new("module-path").module_specific())
            .key(Key::new("database"))
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
            .key(Key::new("database").from_clap("name"))
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
            .key(Key::new("module-path").alias("project-path"))
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

        // Try to exclude a non-existent arg
        let result = CommandSchemaBuilder::new()
            .key(Key::new("server"))
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

        let schema = CommandSchemaBuilder::new().key(Key::new("port")).build(&cmd).unwrap();

        // Config has a string value for port, but clap expects i64
        let mut config_values = HashMap::new();
        config_values.insert("port".to_string(), Value::String("not-a-number".to_string()));

        let command_config = CommandConfig::new(&schema, config_values, &matches).unwrap();

        // Should error when trying to convert invalid value
        let result = command_config.get_one::<i64>("port");
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

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new("database").required())
            .key(Key::new("server"))
            .build(&cmd)
            .unwrap();

        // Config is missing the required "database" key
        let config_values = HashMap::new();
        let command_config = CommandConfig::new(&schema, config_values, &matches).unwrap();

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

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new("database").required())
            .key(Key::new("server"))
            .build(&cmd)
            .unwrap();

        // Config has the required database key
        let mut config_values = HashMap::new();
        config_values.insert("database".to_string(), Value::String("my-db".to_string()));

        let command_config = CommandConfig::new(&schema, config_values, &matches).unwrap();

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

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new().key(Key::new("server")).build(&cmd).unwrap();

        // No required keys, empty config should be fine
        let config_values = HashMap::new();
        let command_config = CommandConfig::new(&schema, config_values, &matches).unwrap();

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
            .key(Key::new("project_path"))
            .key(Key::new("build_options"))
            .build(&cmd)
            .unwrap();

        // Config file has values
        let mut config_values = HashMap::new();
        config_values.insert("project_path".to_string(), Value::String("./my-module".to_string()));
        config_values.insert("build_options".to_string(), Value::String("--release".to_string()));

        let command_config = CommandConfig::new(&schema, config_values, &matches).unwrap();

        // Default values should NOT override config values
        assert_eq!(
            command_config.get_one::<PathBuf>("project_path").unwrap(),
            Some(PathBuf::from("./my-module"))
        );
        assert_eq!(
            command_config.get_one::<String>("build_options").unwrap(),
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
            .key(Key::new("project_path").module_specific())
            .key(Key::new("build_options").module_specific())
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
    fn test_validate_module_specific_uses_user_facing_flag_names() {
        use clap::{Arg, Command};
        use std::path::PathBuf;

        let cmd = Command::new("test").arg(
            Arg::new("wasm_file")
                .long("bin-path")
                .value_parser(clap::value_parser!(PathBuf)),
        );

        let matches = cmd
            .clone()
            .get_matches_from(vec!["test", "--bin-path", "./module.wasm"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new("wasm_file").module_specific())
            .build(&cmd)
            .unwrap();

        let err = schema
            .validate_no_module_specific_cli_args_for_multiple_targets(
                &cmd,
                &matches,
                2,
                "testing multiple targets",
                "Select a single target.",
            )
            .unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("--bin-path"),
            "Expected --bin-path in error, got: {err_msg}"
        );
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
            .key(Key::new("build_options"))
            .build(&cmd)
            .unwrap();

        // Config file uses kebab-case
        let mut config_values = HashMap::new();
        config_values.insert("build-options".to_string(), Value::String("--release".to_string()));

        // The normalization in CommandConfig::new should convert build-options to build_options
        let command_config = CommandConfig::new(&schema, config_values, &matches).unwrap();

        // Should be able to access via snake_case key
        assert_eq!(
            command_config.get_one::<String>("build_options").unwrap(),
            Some("--release".to_string())
        );
    }

    // CommandSchema Tests

    #[test]
    fn test_invalid_clap_reference_caught() {
        let cmd = Command::new("test").arg(
            Arg::new("valid_arg")
                .long("valid-arg")
                .value_parser(clap::value_parser!(String)),
        );

        let result = CommandSchemaBuilder::new().key(Key::new("nonexistent_arg")).build(&cmd);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CommandConfigError::InvalidClapReference { .. }
        ));
    }

    #[test]
    fn test_invalid_alias_reference_caught() {
        let cmd = Command::new("test").arg(Arg::new("name").long("name").value_parser(clap::value_parser!(String)));

        // Reference a valid arg (name) but add invalid alias (nonexistent) via .alias()
        let result = CommandSchemaBuilder::new()
            .key(Key::new("my_key").from_clap("name").alias("nonexistent"))
            .build(&cmd);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, CommandConfigError::InvalidAliasReference { .. }));
    }

    // CommandConfig Tests

    #[test]
    fn test_get_one_returns_none_when_missing_from_both_sources() {
        let cmd = Command::new("test").arg(
            Arg::new("some_arg")
                .long("some-arg")
                .value_parser(clap::value_parser!(String)),
        );

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new("some_arg"))
            .build(&cmd)
            .unwrap();

        let command_config = CommandConfig::new(&schema, HashMap::new(), &matches).unwrap();

        assert_eq!(command_config.get_one::<String>("some_arg").unwrap(), None);
    }

    #[test]
    fn test_get_one_with_aliased_keys() {
        let cmd = Command::new("test").arg(Arg::new("name|identity").value_parser(clap::value_parser!(String)));

        let matches = cmd.clone().get_matches_from(vec!["test", "my-database"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new("database").from_clap("name|identity"))
            .build(&cmd)
            .unwrap();

        let command_config = CommandConfig::new(&schema, HashMap::new(), &matches).unwrap();

        assert_eq!(
            command_config.get_one::<String>("database").unwrap(),
            Some("my-database".to_string())
        );
    }

    #[test]
    fn test_is_from_cli_identifies_sources_correctly() {
        let cmd = Command::new("test")
            .arg(
                Arg::new("cli_arg")
                    .long("cli-arg")
                    .value_parser(clap::value_parser!(String)),
            )
            .arg(
                Arg::new("default_arg")
                    .long("default-arg")
                    .default_value("default")
                    .value_parser(clap::value_parser!(String)),
            )
            .arg(
                Arg::new("config_arg")
                    .long("config-arg")
                    .value_parser(clap::value_parser!(String)),
            );

        let matches = cmd.clone().get_matches_from(vec!["test", "--cli-arg", "from-cli"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new("cli_arg"))
            .key(Key::new("default_arg"))
            .key(Key::new("config_arg"))
            .build(&cmd)
            .unwrap();

        // CLI arg should be detected
        assert!(schema.is_from_cli(&matches, "cli_arg"));

        // Default arg should NOT be detected as CLI
        assert!(!schema.is_from_cli(&matches, "default_arg"));

        // Config arg (not provided anywhere) should NOT be detected as CLI
        assert!(!schema.is_from_cli(&matches, "config_arg"));
    }

    // SpacetimeConfig Tests

    #[test]
    fn test_find_and_load_walks_up_directory_tree() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let subdir1 = root.join("level1");
        let subdir2 = subdir1.join("level2");
        fs::create_dir_all(&subdir2).unwrap();

        // Create config in root
        let config = SpacetimeConfig {
            dev: Some(DevConfig {
                run: Some("test".to_string()),
            }),
            ..Default::default()
        };
        config.save(&root.join("spacetime.json")).unwrap();

        // Search from subdir2 - should find config in root
        let result = SpacetimeConfig::find_and_load_from(subdir2).unwrap();
        assert!(result.is_some());
        let (found_path, found_config) = result.unwrap();
        assert_eq!(found_path, root.join("spacetime.json"));
        assert_eq!(found_config.dev.as_ref().and_then(|d| d.run.as_deref()), Some("test"));
    }

    #[test]
    fn test_malformed_json_returns_error() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("spacetime.json");

        fs::write(&config_path, "{ invalid json }").unwrap();

        let result = SpacetimeConfig::find_and_load_from(temp.path().to_path_buf());
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_file_returns_none() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();

        let result = SpacetimeConfig::find_and_load_from(temp.path().to_path_buf()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_empty_config_file_handled() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("spacetime.json");

        fs::write(&config_path, "{}").unwrap();

        let result = SpacetimeConfig::find_and_load_from(temp.path().to_path_buf()).unwrap();
        assert!(result.is_some());
        let (_, config) = result.unwrap();
        assert!(config.dev.is_none());
        assert!(config.children.is_none());
        assert!(config.generate.is_none());
    }

    #[test]
    fn test_serde_deserialize_u8_from_config() {
        // Verifies that serde_json::from_value handles u8 (num_replicas) correctly,
        // which was broken with the old TypeId-based approach.
        let cmd = Command::new("test").arg(
            Arg::new("num_replicas")
                .long("num-replicas")
                .value_parser(clap::value_parser!(u8)),
        );

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new("num_replicas"))
            .build(&cmd)
            .unwrap();

        let mut config_values = HashMap::new();
        config_values.insert("num_replicas".to_string(), Value::Number(serde_json::Number::from(3u8)));

        let command_config = CommandConfig::new(&schema, config_values, &matches).unwrap();

        assert_eq!(command_config.get_one::<u8>("num_replicas").unwrap(), Some(3u8));
    }

    #[test]
    fn test_serde_deserialize_bool_from_config() {
        // Verifies that bool values (like include_private) can be read from config.
        let cmd = Command::new("test").arg(
            Arg::new("include_private")
                .long("include-private")
                .action(clap::ArgAction::SetTrue),
        );

        let matches = cmd.clone().get_matches_from(vec!["test"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new("include_private"))
            .build(&cmd)
            .unwrap();

        let mut config_values = HashMap::new();
        config_values.insert("include_private".to_string(), Value::Bool(true));

        let command_config = CommandConfig::new(&schema, config_values, &matches).unwrap();

        assert_eq!(command_config.get_one::<bool>("include_private").unwrap(), Some(true));
    }

    #[test]
    fn test_validate_required_key_provided_via_cli_only() {
        // Verifies that validate() passes when a required key is provided
        // via CLI but not in the config file.
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

        let matches = cmd.clone().get_matches_from(vec!["test", "--database", "my-db"]);

        let schema = CommandSchemaBuilder::new()
            .key(Key::new("database").required())
            .key(Key::new("server"))
            .build(&cmd)
            .unwrap();

        // Config is empty - required key "database" is only in CLI
        let command_config = CommandConfig::new(&schema, HashMap::new(), &matches).unwrap();

        // Should pass validation because CLI provides the required key
        assert!(command_config.validate().is_ok());
    }

    #[test]
    fn test_parent_child_inheritance() {
        // Verifies that children inherit unset fields from the parent.
        let json = r#"{
            "database": "parent-db",
            "server": "local",
            "module-path": "./parent-module",
            "build-options": "--release",
            "children": [
                {
                    "database": "child-1",
                    "module-path": "./child-module"
                },
                {
                    "database": "child-2",
                    "module-path": "./child-module",
                    "server": "maincloud"
                }
            ]
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();
        let targets = config.collect_all_targets_with_inheritance();

        // Should have 3 targets: parent + 2 children
        assert_eq!(targets.len(), 3);

        // Parent target
        assert_eq!(
            targets[0].fields.get("database").and_then(|v| v.as_str()),
            Some("parent-db")
        );
        assert_eq!(targets[0].fields.get("server").and_then(|v| v.as_str()), Some("local"));

        // Child 1: inherits server and build-options from parent
        assert_eq!(
            targets[1].fields.get("database").and_then(|v| v.as_str()),
            Some("child-1")
        );
        assert_eq!(
            targets[1].fields.get("server").and_then(|v| v.as_str()),
            Some("local") // inherited from parent
        );
        assert_eq!(
            targets[1].fields.get("build-options").and_then(|v| v.as_str()),
            Some("--release") // inherited from parent
        );

        // Child 2: overrides server, inherits build-options
        assert_eq!(
            targets[2].fields.get("database").and_then(|v| v.as_str()),
            Some("child-2")
        );
        assert_eq!(
            targets[2].fields.get("server").and_then(|v| v.as_str()),
            Some("maincloud") // overridden
        );
        assert_eq!(
            targets[2].fields.get("build-options").and_then(|v| v.as_str()),
            Some("--release") // inherited from parent
        );
    }

    #[test]
    fn test_parent_child_inheritance_no_children() {
        // When there are no children, collect_all_targets_with_inheritance
        // returns just the parent.
        let json = r#"{
            "database": "single-db",
            "server": "local",
            "module-path": "./module"
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();
        let targets = config.collect_all_targets_with_inheritance();

        assert_eq!(targets.len(), 1);
        assert_eq!(
            targets[0].fields.get("database").and_then(|v| v.as_str()),
            Some("single-db")
        );
    }

    #[test]
    fn test_nested_inheritance_grandchildren() {
        // Verifies that inheritance works recursively: grandchildren
        // inherit from their parent (which already inherited from grandparent).
        let json = r#"{
            "server": "production",
            "build-options": "--release",
            "database": "root",
            "children": [
                {
                    "database": "mid",
                    "module-path": "./mid-module",
                    "children": [
                        {
                            "database": "leaf"
                        }
                    ]
                }
            ]
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();
        let targets = config.collect_all_targets_with_inheritance();

        // root + mid + leaf = 3
        assert_eq!(targets.len(), 3);

        // Root
        assert_eq!(targets[0].fields.get("database").and_then(|v| v.as_str()), Some("root"));

        // Mid: inherits server and build-options from root, has own module-path
        assert_eq!(targets[1].fields.get("database").and_then(|v| v.as_str()), Some("mid"));
        assert_eq!(
            targets[1].fields.get("server").and_then(|v| v.as_str()),
            Some("production")
        );
        assert_eq!(
            targets[1].fields.get("module-path").and_then(|v| v.as_str()),
            Some("./mid-module")
        );

        // Leaf: inherits server and build-options (from root via mid),
        // AND inherits module-path from mid
        assert_eq!(targets[2].fields.get("database").and_then(|v| v.as_str()), Some("leaf"));
        assert_eq!(
            targets[2].fields.get("server").and_then(|v| v.as_str()),
            Some("production")
        );
        assert_eq!(
            targets[2].fields.get("build-options").and_then(|v| v.as_str()),
            Some("--release")
        );
        assert_eq!(
            targets[2].fields.get("module-path").and_then(|v| v.as_str()),
            Some("./mid-module")
        );
    }

    #[test]
    fn test_generate_inheritance_from_parent() {
        // Children inherit generate from parent if they don't define their own
        let json = r#"{
            "database": "parent-db",
            "server": "local",
            "generate": [
                { "language": "typescript", "out-dir": "./client/src/bindings" }
            ],
            "children": [
                { "database": "child-1" },
                {
                    "database": "child-2",
                    "generate": [
                        { "language": "csharp", "out-dir": "./csharp-bindings" }
                    ]
                }
            ]
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();
        let targets = config.collect_all_targets_with_inheritance();

        assert_eq!(targets.len(), 3);

        // Parent has its own generate
        let parent_gen = targets[0].generate.as_ref().unwrap();
        assert_eq!(parent_gen.len(), 1);
        assert_eq!(
            parent_gen[0].get("language").and_then(|v| v.as_str()),
            Some("typescript")
        );

        // Child 1 inherits parent's generate
        let child1_gen = targets[1].generate.as_ref().unwrap();
        assert_eq!(child1_gen.len(), 1);
        assert_eq!(
            child1_gen[0].get("language").and_then(|v| v.as_str()),
            Some("typescript")
        );

        // Child 2 overrides with its own generate
        let child2_gen = targets[2].generate.as_ref().unwrap();
        assert_eq!(child2_gen.len(), 1);
        assert_eq!(child2_gen[0].get("language").and_then(|v| v.as_str()), Some("csharp"));
    }

    #[test]
    fn test_find_and_load_with_env_layering() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Base config
        fs::write(
            root.join("spacetime.json"),
            r#"{ "database": "my-db", "server": "local" }"#,
        )
        .unwrap();

        // Dev environment overlay - replaces server
        fs::write(root.join("spacetime.dev.json"), r#"{ "server": "maincloud" }"#).unwrap();

        // Load without env
        let result = find_and_load_with_env_from(None, root.to_path_buf()).unwrap().unwrap();
        assert_eq!(
            result.config.additional_fields.get("server").and_then(|v| v.as_str()),
            Some("local")
        );
        assert!(!result.has_dev_file);

        // Load with dev env
        let result = find_and_load_with_env_from(Some("dev"), root.to_path_buf())
            .unwrap()
            .unwrap();
        assert_eq!(
            result.config.additional_fields.get("server").and_then(|v| v.as_str()),
            Some("maincloud")
        );
        assert_eq!(
            result.config.additional_fields.get("database").and_then(|v| v.as_str()),
            Some("my-db")
        );
        assert!(result.has_dev_file);
        assert_eq!(result.loaded_files.len(), 2);
    }

    #[test]
    fn test_find_and_load_with_env_local_overlay() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Base config
        fs::write(
            root.join("spacetime.json"),
            r#"{ "database": "my-db", "server": "local" }"#,
        )
        .unwrap();

        // Local overlay
        fs::write(root.join("spacetime.local.json"), r#"{ "database": "my-local-db" }"#).unwrap();

        let result = find_and_load_with_env_from(None, root.to_path_buf()).unwrap().unwrap();
        // Local overlay replaces database
        assert_eq!(
            result.config.additional_fields.get("database").and_then(|v| v.as_str()),
            Some("my-local-db")
        );
        // Server is preserved from base
        assert_eq!(
            result.config.additional_fields.get("server").and_then(|v| v.as_str()),
            Some("local")
        );
    }

    #[test]
    fn test_children_overlay_merges_by_index_with_lower_count() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::write(
            root.join("spacetime.json"),
            r#"{
                "database": "root",
                "children": [
                    { "database": "db-a", "server": "base-a" },
                    { "database": "db-b", "server": "base-b" }
                ]
            }"#,
        )
        .unwrap();

        fs::write(
            root.join("spacetime.local.json"),
            r#"{
                "children": [
                    { "server": "local-a", "module-path": "./a" },
                    { "server": "local-b", "module-path": "./b" },
                    { "database": "db-extra", "server": "extra" }
                ]
            }"#,
        )
        .unwrap();

        let result = find_and_load_with_env_from(None, root.to_path_buf()).unwrap().unwrap();
        let children = result.config.children.as_ref().unwrap();
        assert_eq!(children.len(), 2, "child count should remain from base config");

        assert_eq!(
            children[0].additional_fields.get("database").and_then(|v| v.as_str()),
            Some("db-a")
        );
        assert_eq!(
            children[0].additional_fields.get("server").and_then(|v| v.as_str()),
            Some("local-a")
        );
        assert_eq!(
            children[0]
                .additional_fields
                .get("module-path")
                .and_then(|v| v.as_str()),
            Some("./a")
        );

        assert_eq!(
            children[1].additional_fields.get("database").and_then(|v| v.as_str()),
            Some("db-b")
        );
        assert_eq!(
            children[1].additional_fields.get("server").and_then(|v| v.as_str()),
            Some("local-b")
        );
        assert_eq!(
            children[1]
                .additional_fields
                .get("module-path")
                .and_then(|v| v.as_str()),
            Some("./b")
        );
    }

    #[test]
    fn test_children_overlay_merges_recursively_for_nested_children() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::write(
            root.join("spacetime.json"),
            r#"{
                "database": "root",
                "children": [
                    {
                        "database": "parent-a",
                        "children": [
                            { "database": "grand-a1", "server": "base-g1" },
                            { "database": "grand-a2", "server": "base-g2" }
                        ]
                    }
                ]
            }"#,
        )
        .unwrap();

        fs::write(
            root.join("spacetime.local.json"),
            r#"{
                "children": [
                    {
                        "children": [
                            { "server": "local-g1", "module-path": "./nested" }
                        ]
                    }
                ]
            }"#,
        )
        .unwrap();

        let result = find_and_load_with_env_from(None, root.to_path_buf()).unwrap().unwrap();
        let children = result.config.children.as_ref().unwrap();
        let grandchildren = children[0].children.as_ref().unwrap();
        assert_eq!(grandchildren.len(), 2);

        assert_eq!(
            grandchildren[0]
                .additional_fields
                .get("database")
                .and_then(|v| v.as_str()),
            Some("grand-a1")
        );
        assert_eq!(
            grandchildren[0]
                .additional_fields
                .get("server")
                .and_then(|v| v.as_str()),
            Some("local-g1")
        );
        assert_eq!(
            grandchildren[0]
                .additional_fields
                .get("module-path")
                .and_then(|v| v.as_str()),
            Some("./nested")
        );

        assert_eq!(
            grandchildren[1]
                .additional_fields
                .get("database")
                .and_then(|v| v.as_str()),
            Some("grand-a2")
        );
        assert_eq!(
            grandchildren[1]
                .additional_fields
                .get("server")
                .and_then(|v| v.as_str()),
            Some("base-g2")
        );
    }

    #[test]
    fn test_source_config_tracks_database_origin_per_target() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::write(
            root.join("spacetime.json"),
            r#"{
                "database": "root-base",
                "children": [
                    { "database": "child-a-base", "server": "base-a" },
                    { "database": "child-b-base", "server": "base-b" }
                ]
            }"#,
        )
        .unwrap();

        fs::write(
            root.join("spacetime.local.json"),
            r#"{
                "database": "root-local",
                "children": [
                    { "database": "child-a-local" },
                    { "server": "only-server-override" }
                ]
            }"#,
        )
        .unwrap();

        fs::write(
            root.join("spacetime.dev.json"),
            r#"{
                "children": [
                    { "server": "dev-a" },
                    { "database": "child-b-dev" }
                ]
            }"#,
        )
        .unwrap();

        let result = find_and_load_with_env_from(Some("dev"), root.to_path_buf())
            .unwrap()
            .unwrap();

        assert_eq!(result.config.source_config.as_deref(), Some("spacetime.local.json"));

        let children = result.config.children.as_ref().unwrap();
        assert_eq!(children[0].source_config.as_deref(), Some("spacetime.local.json"));
        assert_eq!(children[1].source_config.as_deref(), Some("spacetime.dev.json"));
    }

    #[test]
    fn test_source_config_not_inherited_to_children_without_database() {
        let json = r#"{
            "database": "root-db",
            "_source-config": "spacetime.local.json",
            "children": [
                { "server": "local" }
            ]
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();
        assert_eq!(config.source_config.as_deref(), Some("spacetime.local.json"));
        let child = config.children.as_ref().unwrap().first().unwrap();
        assert!(
            child.source_config.is_none(),
            "_source-config should not be inherited to children"
        );
    }

    #[test]
    fn test_multi_level_env_layering_staging() {
        // Full overlay order: base → local → staging → staging.local
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Base config
        fs::write(
            root.join("spacetime.json"),
            r#"{ "database": "base-db", "server": "local", "module-path": "./server" }"#,
        )
        .unwrap();

        // Staging env overlay (applies after local)
        fs::write(
            root.join("spacetime.staging.json"),
            r#"{ "server": "staging-server", "database": "staging-db" }"#,
        )
        .unwrap();

        // Local overlay (applies before env)
        fs::write(
            root.join("spacetime.local.json"),
            r#"{ "database": "local-override-db" }"#,
        )
        .unwrap();

        // Staging local overlay (applies last)
        fs::write(
            root.join("spacetime.staging.local.json"),
            r#"{ "database": "staging-local-db" }"#,
        )
        .unwrap();

        let result = find_and_load_with_env_from(Some("staging"), root.to_path_buf())
            .unwrap()
            .unwrap();

        // database: base-db → local-override-db → staging-db → staging-local-db
        assert_eq!(
            result.config.additional_fields.get("database").and_then(|v| v.as_str()),
            Some("staging-local-db")
        );
        // server: local → staging-server (not overridden by local files)
        assert_eq!(
            result.config.additional_fields.get("server").and_then(|v| v.as_str()),
            Some("staging-server")
        );
        // module-path: only in base, preserved through all overlays
        assert_eq!(
            result
                .config
                .additional_fields
                .get("module-path")
                .and_then(|v| v.as_str()),
            Some("./server")
        );
        // 4 files loaded
        assert_eq!(result.loaded_files.len(), 4);
        assert_eq!(
            result.loaded_files[1].file_name().and_then(|s| s.to_str()),
            Some("spacetime.local.json")
        );
        assert_eq!(
            result.loaded_files[2].file_name().and_then(|s| s.to_str()),
            Some("spacetime.staging.json")
        );
    }

    #[test]
    fn test_has_dev_file_false_for_non_dev_env() {
        // has_dev_file should only be true for env="dev", not for other envs
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::write(root.join("spacetime.json"), r#"{ "database": "my-db" }"#).unwrap();

        fs::write(root.join("spacetime.staging.json"), r#"{ "server": "staging" }"#).unwrap();

        let result = find_and_load_with_env_from(Some("staging"), root.to_path_buf())
            .unwrap()
            .unwrap();
        assert!(!result.has_dev_file, "has_dev_file should be false for staging env");

        // But dev env should set it
        fs::write(root.join("spacetime.dev.json"), r#"{ "server": "local" }"#).unwrap();

        let result = find_and_load_with_env_from(Some("dev"), root.to_path_buf())
            .unwrap()
            .unwrap();
        assert!(result.has_dev_file, "has_dev_file should be true for dev env");
    }

    #[test]
    fn test_dev_not_propagated_to_children() {
        // dev is root-only and should NOT appear in child targets
        let json = r#"{
            "database": "parent-db",
            "server": "local",
            "dev": { "run": "npm run dev" },
            "children": [
                { "database": "child-db" }
            ]
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();
        let targets = config.collect_all_targets_with_inheritance();

        assert_eq!(targets.len(), 2);

        // Parent should have database and server in fields
        assert_eq!(
            targets[0].fields.get("database").and_then(|v| v.as_str()),
            Some("parent-db")
        );

        // Child should inherit server but NOT have dev in fields
        assert_eq!(
            targets[1].fields.get("database").and_then(|v| v.as_str()),
            Some("child-db")
        );
        assert_eq!(targets[1].fields.get("server").and_then(|v| v.as_str()), Some("local"));
        // dev should not be in additional_fields of FlatTarget
        assert!(
            !targets[1].fields.contains_key("dev"),
            "dev should not be propagated to children via additional_fields"
        );
        // Also verify parent's flat target doesn't leak dev into fields
        assert!(
            !targets[0].fields.contains_key("dev"),
            "dev should not appear in FlatTarget fields (it's a typed field, not in additional_fields)"
        );
    }

    #[test]
    fn test_generate_dedup_with_inherited_generate() {
        // Two sibling databases sharing parent's generate + same module path
        // should deduplicate to a single generate entry
        let json = r#"{
            "module-path": "./server",
            "generate": [
                { "language": "typescript", "out-dir": "./client/src/bindings" }
            ],
            "children": [
                { "database": "region-1" },
                { "database": "region-2" }
            ]
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();
        let targets = config.collect_all_targets_with_inheritance();

        // All 3 targets (parent + 2 children) share the same module-path and generate
        assert_eq!(targets.len(), 3);
        for target in &targets {
            assert_eq!(
                target.fields.get("module-path").and_then(|v| v.as_str()),
                Some("./server")
            );
            let gen = target.generate.as_ref().unwrap();
            assert_eq!(gen.len(), 1);
            assert_eq!(gen[0].get("language").and_then(|v| v.as_str()), Some("typescript"));
        }

        // All have the same (module-path, generate) so dedup should reduce to 1
        // (this is verified in generate.rs tests, but we confirm the data here)
    }

    #[test]
    fn test_iter_all_targets_includes_self_and_descendants() {
        let json = r#"{
            "database": "root",
            "children": [
                {
                    "database": "mid",
                    "children": [
                        { "database": "leaf" }
                    ]
                }
            ]
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();
        let all: Vec<_> = config.iter_all_targets().collect();
        assert_eq!(all.len(), 3);
        assert_eq!(
            all[0].additional_fields.get("database").and_then(|v| v.as_str()),
            Some("root")
        );
        assert_eq!(
            all[1].additional_fields.get("database").and_then(|v| v.as_str()),
            Some("mid")
        );
        assert_eq!(
            all[2].additional_fields.get("database").and_then(|v| v.as_str()),
            Some("leaf")
        );
    }

    #[test]
    fn test_count_targets() {
        let json = r#"{
            "database": "root",
            "children": [
                { "database": "child-1" },
                {
                    "database": "child-2",
                    "children": [
                        { "database": "grandchild" }
                    ]
                }
            ]
        }"#;

        let config: SpacetimeConfig = json5::from_str(json).unwrap();
        assert_eq!(config.count_targets(), 4); // root + child-1 + child-2 + grandchild
    }

    #[test]
    fn test_path_clean_preserves_leading_dotdot() {
        // Regression test for #4429: leading `..` must be preserved.
        // All config paths (--out-dir, --module-path, etc.) go through
        // get_resolved_path which calls PathClean::clean().
        use path_clean::PathClean;
        use std::path::Path;

        // --out-dir cases
        assert_eq!(Path::new("../foo").clean(), PathBuf::from("../foo"));
        assert_eq!(Path::new("../../a/b").clean(), PathBuf::from("../../a/b"));
        assert_eq!(
            Path::new("../frontend-ts-src/module-bindings").clean(),
            PathBuf::from("../frontend-ts-src/module-bindings")
        );
        // Inner `..` should still resolve.
        assert_eq!(Path::new("a/b/../c").clean(), PathBuf::from("a/c"));
        // Pure `..` should stay.
        assert_eq!(Path::new("..").clean(), PathBuf::from(".."));
        // Absolute paths
        assert_eq!(
            Path::new("/home/user/project/../foo").clean(),
            PathBuf::from("/home/user/foo")
        );
        // Current dir collapses.
        assert_eq!(Path::new("./foo").clean(), PathBuf::from("foo"));
        // Empty result → "."
        assert_eq!(Path::new(".").clean(), PathBuf::from("."));
        assert_eq!(Path::new("a/..").clean(), PathBuf::from("."));

        // --module-path cases (same bug, reported by user on #4431)
        assert_eq!(Path::new("../server").clean(), PathBuf::from("../server"));
        assert_eq!(
            Path::new("../../repos/server").clean(),
            PathBuf::from("../../repos/server")
        );
        assert_eq!(
            Path::new("../repos/server/spacetimedb").clean(),
            PathBuf::from("../repos/server/spacetimedb")
        );
    }
}
