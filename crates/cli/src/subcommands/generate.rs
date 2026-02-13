#![warn(clippy::uninlined_format_args)]

use anyhow::Context;
use clap::parser::ValueSource;
use clap::Arg;
use clap::ArgAction::{Set, SetTrue};
use fs_err as fs;
use spacetimedb_codegen::{
    generate, private_table_names, CodegenOptions, CodegenVisibility, Csharp, Lang, OutputFile, Rust, TypeScript,
    UnrealCpp, AUTO_GENERATED_PREFIX,
};
use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::{sats, RawModuleDef};
use spacetimedb_schema;
use spacetimedb_schema::def::ModuleDef;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::spacetime_config::{CommandConfig, CommandSchema, CommandSchemaBuilder, Key, SpacetimeConfig};
use crate::tasks::csharp::dotnet_format;
use crate::tasks::rust::rustfmt;
use crate::util::{find_module_path, resolve_sibling_binary, y_or_n};
use crate::Config;
use crate::{build, common_args};
use clap::builder::PossibleValue;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::io::Read;

/// Build the CommandSchema for generate command configuration.
///
/// This schema is used to validate and merge values from both the config file
/// and CLI arguments, with CLI arguments taking precedence over config values.
fn build_generate_config_schema(command: &clap::Command) -> Result<CommandSchema, anyhow::Error> {
    CommandSchemaBuilder::new()
        .key(
            Key::new::<Language>("language")
                .from_clap("lang")
                .required()
                .module_specific(),
        )
        .key(Key::new::<PathBuf>("out_dir").module_specific())
        .key(Key::new::<PathBuf>("uproject_dir").module_specific())
        .key(Key::new::<PathBuf>("module_path").module_specific())
        .key(Key::new::<PathBuf>("wasm_file").module_specific())
        .key(Key::new::<PathBuf>("js_file").module_specific())
        .key(Key::new::<String>("namespace").module_specific())
        .key(Key::new::<String>("module_name").module_specific())
        .key(Key::new::<String>("build_options").module_specific())
        .key(Key::new::<String>("include_private"))
        .exclude("json_module")
        .exclude("force")
        .build(command)
        .map_err(Into::into)
}

/// Get filtered generate configs based on CLI arguments. When the user sets
/// the module path as a CLI argument and the config file is available,
/// we should only run the generate command for config entries that match
/// the module path
fn get_filtered_generate_configs<'a>(
    spacetime_config: &'a SpacetimeConfig,
    command: &clap::Command,
    schema: &'a CommandSchema,
    args: &'a clap::ArgMatches,
) -> Result<Vec<CommandConfig<'a>>, anyhow::Error> {
    // Get all generate configs from spacetime.json
    let all_configs: Vec<HashMap<String, Value>> = spacetime_config.generate.as_ref().cloned().unwrap_or_default();

    // If no config file, return empty (will use CLI args only)
    if all_configs.is_empty() {
        return Ok(vec![]);
    }

    // Build CommandConfig for each generate config - this merges any arguments passed
    // through the CLI with the values from the config file
    let all_command_configs: Vec<CommandConfig> = all_configs
        .into_iter()
        .map(|config| {
            let command_config = CommandConfig::new(schema, config, args)?;
            command_config.validate()?;
            Ok(command_config)
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()?;

    // Filter by module_path if provided via CLI
    let filtered_configs: Vec<CommandConfig> = if schema.is_from_cli(args, "module_path") {
        let cli_module_path = schema.get_clap_arg::<PathBuf>(args, "module_path")?;
        // Canonicalize the CLI path for comparison (if it exists)
        let cli_canonical = cli_module_path.as_ref().and_then(|p| p.canonicalize().ok());

        all_command_configs
            .into_iter()
            .filter(|config| {
                // Get module_path from CONFIG ONLY (not merged with CLI)
                let config_module_path = config
                    .get_config_value("module_path")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from);

                // If we have a canonical CLI path, try to canonicalize config path and compare
                if let Some(ref cli_canon) = cli_canonical {
                    if let Some(ref config_path) = config_module_path {
                        if let Ok(config_canon) = config_path.canonicalize() {
                            return cli_canon == &config_canon;
                        }
                    }
                }

                // Fallback to direct comparison if canonicalization fails
                config_module_path.as_ref() == cli_module_path.as_ref()
            })
            .collect()
    } else {
        all_command_configs
    };

    schema.validate_no_module_specific_cli_args_for_multiple_targets(
        command,
        args,
        filtered_configs.len(),
        "generating for multiple targets",
        "Please specify --module-path to select a single target, or remove these arguments.",
    )?;

    Ok(filtered_configs)
}

pub fn cli() -> clap::Command {
    clap::Command::new("generate")
        .about("Generate client files for a spacetime module.")
        .override_usage("spacetime generate --lang <LANG> --out-dir <DIR> [--module-path <DIR> | --bin-path <PATH> | --module-name <MODULE_NAME> | --uproject-dir <DIR> | --include-private]")
        .arg(
            Arg::new("wasm_file")
                .value_parser(clap::value_parser!(PathBuf))
                .long("bin-path")
                .short('b')
                .group("source")
                .conflicts_with("module_path")
                .conflicts_with("build_options")
                .help("The system path (absolute or relative) to the compiled wasm binary we should inspect"),
        )
        .arg(
            Arg::new("js_file")
                .value_parser(clap::value_parser!(PathBuf))
                .long("js-path")
                .short('j')
                .group("source")
                .conflicts_with("module_path")
                .conflicts_with("build_options")
                .help("The system path (absolute or relative) to the bundled javascript file we should inspect"),
        )
        .arg(
            Arg::new("module_path")
                .value_parser(clap::value_parser!(PathBuf))
                .long("module-path")
                .short('p')
                .group("source")
                .help("The system path (absolute or relative) to the module project. Defaults to spacetimedb/ subdirectory, then current directory."),
        )
        .arg(
            Arg::new("json_module")
                .hide(true)
                .num_args(0..=1)
                .value_parser(clap::value_parser!(PathBuf))
                .long("module-def")
                .group("source")
                .help("Generate from a ModuleDef encoded as json"),
        )
        .arg(
            Arg::new("out_dir")
                .value_parser(clap::value_parser!(PathBuf))
                .long("out-dir")
                .short('o')
                .help("The system path (absolute or relative) to the generate output directory"),
        )
        .arg(
            Arg::new("uproject_dir")
                .value_parser(clap::value_parser!(PathBuf))
                .long("uproject-dir")
                .help("Path to the Unreal project directory, replaces --out-dir for Unreal generation (only used with --lang unrealcpp)")
        )
        .arg(
            Arg::new("namespace")
                .default_value("SpacetimeDB.Types")
                .long("namespace")
                .help("The namespace that should be used"),
        )
        .arg(
            Arg::new("module_name")
                .long("module-name")
                .help("The module name that should be used for DLL export macros (required for lang unrealcpp)")
        )
        .arg(
            Arg::new("lang")
                .long("lang")
                .short('l')
                .value_parser(clap::value_parser!(Language))
                .help("The language to generate"),
        )
        .arg(
            Arg::new("build_options")
                .long("build-options")
                .alias("build-opts")
                .action(Set)
                .default_value("")
                .help("Options to pass to the build command, for example --build-options='--lint-dir='"),
        )
        .arg(
            Arg::new("include_private")
                .long("include-private")
                .action(SetTrue)
                .default_value("false")
                .help("Include private tables and functions in generated code (types are always included)."),
        )
        .arg(common_args::yes())
        .after_help("Run `spacetime help publish` for more detailed information.")
}

pub async fn exec(config: Config, args: &clap::ArgMatches) -> anyhow::Result<()> {
    exec_ex(config, args, extract_descriptions, false).await
}

/// Like `exec`, but lets you specify a custom a function to extract a schema from a file.
pub async fn exec_ex(
    config: Config,
    args: &clap::ArgMatches,
    extract_descriptions: ExtractDescriptions,
    quiet_config: bool,
) -> anyhow::Result<()> {
    // Build schema
    let cmd = cli();
    let schema = build_generate_config_schema(&cmd)?;

    // Get generate configs (from spacetime.json or empty)
    let spacetime_config_opt = SpacetimeConfig::find_and_load()?;
    let (using_config, generate_configs) = if let Some((config_path, ref spacetime_config)) = spacetime_config_opt {
        if !quiet_config {
            println!("Using configuration from {}", config_path.display());
        }
        let filtered = get_filtered_generate_configs(spacetime_config, &cmd, &schema, args)?;
        // If filtering resulted in no matches, use CLI args with empty config
        if filtered.is_empty() {
            (false, vec![CommandConfig::new(&schema, HashMap::new(), args)?])
        } else {
            (true, filtered)
        }
    } else {
        (false, vec![CommandConfig::new(&schema, HashMap::new(), args)?])
    };

    // Execute generate for each config
    for command_config in generate_configs {
        // Get values using command_config.get_one() which merges CLI + config
        let project_path = match command_config.get_one::<PathBuf>("module_path")? {
            Some(path) => path,
            None if using_config => {
                anyhow::bail!("module-path must be specified for each generate target when using spacetime.json");
            }
            None => find_module_path(&std::env::current_dir()?).ok_or_else(|| {
                anyhow::anyhow!(
                    "Could not find a SpacetimeDB module in spacetimedb/ or the current directory. \
                     Use --module-path to specify the module location."
                )
            })?,
        };
        let wasm_file = command_config.get_one::<PathBuf>("wasm_file")?;
        let js_file = command_config.get_one::<PathBuf>("js_file")?;
        let json_module = args.get_many::<PathBuf>("json_module");
        let lang = command_config
            .get_one::<Language>("language")?
            .ok_or_else(|| anyhow::anyhow!("Language is required (use --lang or add to config)"))?;

        println!(
            "Generating {} module bindings for module {}",
            lang.display_name(),
            project_path.display()
        );

        let namespace = command_config
            .get_one::<String>("namespace")?
            .unwrap_or_else(|| "SpacetimeDB.Types".to_string());
        let module_name = command_config.get_one::<String>("module_name")?;
        let force = args.get_flag("force");
        let build_options = command_config
            .get_one::<String>("build_options")?
            .unwrap_or_else(String::new);

        // Validate namespace is only used with csharp
        if args.value_source("namespace") == Some(ValueSource::CommandLine) && lang != Language::Csharp {
            return Err(anyhow::anyhow!("--namespace is only supported with --lang csharp"));
        }

        // Get output directory (either out_dir or uproject_dir)
        let out_dir = command_config
            .get_one::<PathBuf>("out_dir")?
            .or_else(|| command_config.get_one::<PathBuf>("uproject_dir").ok().flatten())
            .ok_or_else(|| anyhow::anyhow!("Either --out-dir or --uproject-dir is required"))?;

        // Validate language-specific requirements
        match lang {
            Language::Rust | Language::Csharp | Language::TypeScript => {
                // These languages require out_dir (not uproject_dir)
                if command_config.get_one::<PathBuf>("out_dir")?.is_none() {
                    return Err(anyhow::anyhow!(
                        "--out-dir is required for --lang {}",
                        match lang {
                            Language::Rust => "rust",
                            Language::Csharp => "csharp",
                            Language::TypeScript => "typescript",
                            _ => unreachable!(),
                        }
                    ));
                }
            }
            Language::UnrealCpp => {
                // UnrealCpp requires uproject_dir and module_name
                if command_config.get_one::<PathBuf>("uproject_dir")?.is_none() {
                    return Err(anyhow::anyhow!("--uproject-dir is required for --lang unrealcpp"));
                }
                if module_name.is_none() {
                    return Err(anyhow::anyhow!("--module-name is required for --lang unrealcpp"));
                }
            }
        }

        let module: ModuleDef = if let Some(mut json_module) = json_module {
            let DeserializeWrapper::<RawModuleDef>(module) = if let Some(path) = json_module.next() {
                serde_json::from_slice(&fs::read(path)?)?
            } else {
                serde_json::from_reader(std::io::stdin().lock())?
            };
            module.try_into()?
        } else {
            let path = if let Some(path) = wasm_file {
                println!("Skipping build. Instead we are inspecting {}", path.display());
                path.clone()
            } else if let Some(path) = js_file {
                println!("Skipping build. Instead we are inspecting {}", path.display());
                path.clone()
            } else {
                let (path, _) = build::exec_with_argstring(config.clone(), &project_path, &build_options).await?;
                path
            };
            let spinner = indicatif::ProgressBar::new_spinner();
            spinner.enable_steady_tick(std::time::Duration::from_millis(60));
            spinner.set_message(format!("Extracting schema from {}...", path.display()));
            extract_descriptions(&path).context("could not extract schema")?
        };

        fs::create_dir_all(&out_dir)?;

        let mut paths = BTreeSet::new();

        let include_private = args.get_flag("include_private");
        let private_tables = private_table_names(&module);
        if !private_tables.is_empty() && !include_private {
            println!("Skipping private tables during codegen: {}.", private_tables.join(", "));
        }
        let mut options = CodegenOptions::default();
        if include_private {
            options.visibility = CodegenVisibility::IncludePrivate;
        }

        let csharp_lang;
        let unreal_cpp_lang;
        let gen_lang = match lang {
            Language::Csharp => {
                csharp_lang = Csharp { namespace: &namespace };
                &csharp_lang as &dyn Lang
            }
            Language::UnrealCpp => {
                unreal_cpp_lang = UnrealCpp {
                    module_name: module_name.as_ref().unwrap(),
                    uproject_dir: &out_dir,
                };
                &unreal_cpp_lang as &dyn Lang
            }
            Language::Rust => &Rust,
            Language::TypeScript => &TypeScript,
        };

        for OutputFile { filename, code } in generate(&module, gen_lang, &options) {
            let fname = Path::new(&filename);
            // If a generator asks for a file in a subdirectory, create the subdirectory first.
            if let Some(parent) = fname.parent().filter(|p| !p.as_os_str().is_empty()) {
                println!("Creating directory {}", out_dir.join(parent).display());
                fs::create_dir_all(out_dir.join(parent))?;
            }
            let path = out_dir.join(fname);
            if !path.exists() || fs::read_to_string(&path)? != code {
                println!("Writing file {}", path.display());
                fs::write(&path, code)?;
            }
            paths.insert(path);
        }

        // For Unreal, we want to clean up just the module directory, not the entire uproject directory tree.
        let cleanup_root = match lang {
            Language::UnrealCpp => out_dir.join("Source").join(module_name.as_ref().unwrap()),
            _ => out_dir.clone(),
        };

        // TODO: We should probably just delete all generated files before we generate any, rather than selectively deleting some afterward.
        let mut auto_generated_buf: [u8; AUTO_GENERATED_PREFIX.len()] = [0; AUTO_GENERATED_PREFIX.len()];
        let files_to_delete = walkdir::WalkDir::new(&cleanup_root)
            .into_iter()
            .map(|entry_result| {
                let entry = entry_result?;
                // Only delete files.
                if !entry.file_type().is_file() {
                    return Ok(None);
                }
                let path = entry.into_path();
                // Don't delete regenerated files.
                if paths.contains(&path) {
                    return Ok(None);
                }
                // Only delete files that start with the auto-generated prefix.
                let mut file = fs::File::open(&path)?;
                Ok(match file.read_exact(&mut auto_generated_buf) {
                    Ok(()) => (auto_generated_buf == AUTO_GENERATED_PREFIX.as_bytes()).then_some(path),
                    Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => None,
                    Err(err) => return Err(err.into()),
                })
            })
            .filter_map(Result::transpose)
            .collect::<anyhow::Result<Vec<_>>>()?;

        if !files_to_delete.is_empty() {
            println!("The following files were not generated by this command and will be deleted:");
            for path in &files_to_delete {
                println!("  {}", path.to_str().unwrap());
            }

            if y_or_n(force, "Are you sure you want to delete these files?")? {
                for path in files_to_delete {
                    fs::remove_file(path)?;
                }
                println!("Files deleted successfully.");
            } else {
                println!("Files not deleted.");
            }
        }

        if let Err(err) = lang.format_files(&out_dir, paths) {
            // If we couldn't format the files, print a warning but don't fail the entire
            // task as the output should still be usable, just less pretty.
            eprintln!("Could not format generated files: {err}");
        }

        println!("Generate finished successfully.");
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Language {
    Csharp,
    TypeScript,
    Rust,
    UnrealCpp,
}

impl clap::ValueEnum for Language {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Csharp, Self::TypeScript, Self::Rust, Self::UnrealCpp]
    }
    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(match self {
            Self::Csharp => clap::builder::PossibleValue::new("csharp").aliases(["c#", "cs"]),
            Self::TypeScript => clap::builder::PossibleValue::new("typescript").aliases(["ts", "TS"]),
            Self::Rust => clap::builder::PossibleValue::new("rust").aliases(["rs", "RS"]),
            Self::UnrealCpp => PossibleValue::new("unrealcpp").aliases(["uecpp", "ue5cpp", "unreal"]),
        })
    }
}

impl Language {
    /// Returns the display name for the language
    pub fn display_name(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::Csharp => "C#",
            Language::TypeScript => "TypeScript",
            Language::UnrealCpp => "Unreal C++",
        }
    }

    fn format_files(&self, project_dir: &Path, generated_files: BTreeSet<PathBuf>) -> anyhow::Result<()> {
        match self {
            Language::Rust => rustfmt(generated_files)?,
            Language::Csharp => dotnet_format(project_dir, generated_files)?,
            Language::TypeScript => {
                // TODO: implement formatting.
            }
            Language::UnrealCpp => {
                // TODO: implement formatting.
            }
        }

        Ok(())
    }
}

pub type ExtractDescriptions = fn(&Path) -> anyhow::Result<ModuleDef>;
pub fn extract_descriptions(wasm_file: &Path) -> anyhow::Result<ModuleDef> {
    let bin_path = resolve_sibling_binary("spacetimedb-standalone")?;
    let child = Command::new(&bin_path)
        .arg("extract-schema")
        .arg(wasm_file)
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn {}", bin_path.display()))?;
    let sats::serde::SerdeWrapper::<RawModuleDef>(module) = serde_json::from_reader(child.stdout.unwrap())?;
    Ok(module.try_into()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacetime_config::*;
    use std::collections::HashMap;

    // get_filtered_generate_configs Tests

    #[test]
    fn test_filter_by_module_path_from_cli() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        let module1 = temp.path().join("module1");
        let module2 = temp.path().join("module2");
        std::fs::create_dir_all(&module1).unwrap();
        std::fs::create_dir_all(&module2).unwrap();

        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();

        let mut config1 = HashMap::new();
        config1.insert("language".to_string(), serde_json::Value::String("rust".to_string()));
        config1.insert(
            "module_path".to_string(),
            serde_json::Value::String(module1.display().to_string()),
        );
        config1.insert(
            "out_dir".to_string(),
            serde_json::Value::String("/tmp/out1".to_string()),
        );

        let mut config2 = HashMap::new();
        config2.insert(
            "language".to_string(),
            serde_json::Value::String("typescript".to_string()),
        );
        config2.insert(
            "module_path".to_string(),
            serde_json::Value::String(module2.display().to_string()),
        );
        config2.insert(
            "out_dir".to_string(),
            serde_json::Value::String("/tmp/out2".to_string()),
        );

        let spacetime_config = SpacetimeConfig {
            generate: Some(vec![config1, config2]),
            ..Default::default()
        };

        // Filter by module1
        let matches = cmd.clone().get_matches_from(vec![
            "generate",
            "--module-path",
            module1.to_str().unwrap(),
            "--lang",
            "rust",
            "--out-dir",
            "/tmp/out",
        ]);

        let filtered = get_filtered_generate_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        // The filtering should match module1 config only
        assert_eq!(
            filtered.len(),
            1,
            "Expected 1 config but got {}. Filter should only match module1.",
            filtered.len()
        );

        // Verify it's the correct config (module1)
        let filtered_module_path = filtered[0].get_one::<PathBuf>("module_path").unwrap().unwrap();
        assert_eq!(filtered_module_path, module1);
    }

    #[test]
    fn test_no_filter_when_module_path_not_from_cli() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();

        let mut config1 = HashMap::new();
        config1.insert("language".to_string(), serde_json::Value::String("rust".to_string()));
        config1.insert(
            "module_path".to_string(),
            serde_json::Value::String("./module1".to_string()),
        );
        config1.insert(
            "out_dir".to_string(),
            serde_json::Value::String("/tmp/out1".to_string()),
        );

        let mut config2 = HashMap::new();
        config2.insert(
            "language".to_string(),
            serde_json::Value::String("typescript".to_string()),
        );
        config2.insert(
            "module_path".to_string(),
            serde_json::Value::String("./module2".to_string()),
        );
        config2.insert(
            "out_dir".to_string(),
            serde_json::Value::String("/tmp/out2".to_string()),
        );

        let spacetime_config = SpacetimeConfig {
            generate: Some(vec![config1, config2]),
            ..Default::default()
        };

        // No module_path provided via CLI
        let matches = cmd.clone().get_matches_from(vec!["generate"]);
        let filtered = get_filtered_generate_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        // Should return all configs
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_path_normalization_in_filtering() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        let module_dir = temp.path().join("mymodule");
        std::fs::create_dir_all(&module_dir).unwrap();

        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();

        // Config uses absolute path
        let mut config = HashMap::new();
        config.insert("language".to_string(), serde_json::Value::String("rust".to_string()));
        config.insert(
            "module_path".to_string(),
            serde_json::Value::String(module_dir.display().to_string()),
        );
        config.insert("out_dir".to_string(), serde_json::Value::String("/tmp/out".to_string()));

        let spacetime_config = SpacetimeConfig {
            generate: Some(vec![config]),
            ..Default::default()
        };

        // CLI uses path with ./ and ..
        let cli_path = module_dir.join("..").join("mymodule");
        let matches = cmd.clone().get_matches_from(vec![
            "generate",
            "--module-path",
            cli_path.to_str().unwrap(),
            "--lang",
            "rust",
            "--out-dir",
            "/tmp/out",
        ]);
        let filtered = get_filtered_generate_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        // Should match despite different path representations
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_module_specific_args_error_with_multiple_targets() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();

        let mut config1 = HashMap::new();
        config1.insert("language".to_string(), serde_json::Value::String("rust".to_string()));
        config1.insert(
            "module_path".to_string(),
            serde_json::Value::String("./module1".to_string()),
        );
        config1.insert(
            "out_dir".to_string(),
            serde_json::Value::String("/tmp/out1".to_string()),
        );

        let mut config2 = HashMap::new();
        config2.insert(
            "language".to_string(),
            serde_json::Value::String("typescript".to_string()),
        );
        config2.insert(
            "module_path".to_string(),
            serde_json::Value::String("./module2".to_string()),
        );
        config2.insert(
            "out_dir".to_string(),
            serde_json::Value::String("/tmp/out2".to_string()),
        );

        let spacetime_config = SpacetimeConfig {
            generate: Some(vec![config1, config2]),
            ..Default::default()
        };

        let matches = cmd
            .clone()
            .get_matches_from(vec!["generate", "--out-dir", "/tmp/override"]);
        let err = get_filtered_generate_configs(&spacetime_config, &cmd, &schema, &matches).unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("--out-dir"),
            "Expected error to mention --out-dir, got: {err_msg}"
        );
        assert!(
            err_msg.contains("multiple targets"),
            "Expected error to mention multiple targets, got: {err_msg}"
        );
    }

    // Language-Specific Validation Tests

    #[tokio::test]
    async fn test_rust_requires_out_dir() {
        use crate::config::Config;
        use spacetimedb_paths::cli::CliTomlPath;
        use spacetimedb_paths::FromPathUnchecked;

        let cmd = cli();
        let config = Config::new_with_localhost(CliTomlPath::from_path_unchecked("/tmp/test-config.toml"));

        // Missing --out-dir for rust
        let matches = cmd.clone().get_matches_from(vec!["generate", "--lang", "rust"]);
        let result = exec(config, &matches).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        // The error should be about missing output directory
        assert!(
            err_msg.contains("--out-dir") || err_msg.contains("--uproject-dir"),
            "Expected error about missing output directory, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_unrealcpp_requires_uproject_dir_and_module_name() {
        use crate::config::Config;
        use spacetimedb_paths::cli::CliTomlPath;
        use spacetimedb_paths::FromPathUnchecked;

        let cmd = cli();
        let config = Config::new_with_localhost(CliTomlPath::from_path_unchecked("/tmp/test-config.toml"));

        // Test missing --uproject-dir
        let matches =
            cmd.clone()
                .get_matches_from(vec!["generate", "--lang", "unrealcpp", "--module-name", "MyModule"]);
        let result = exec(config.clone(), &matches).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("--uproject-dir is required for --lang unrealcpp"),
            "Expected error about missing --uproject-dir, got: {err_msg}",
        );

        // Test missing --module-name
        let matches = cmd
            .clone()
            .get_matches_from(vec!["generate", "--lang", "unrealcpp", "--out-dir", "/tmp/out"]);
        let result = exec(config, &matches).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("--module-name is required for --lang unrealcpp"),
            "Expected error about missing --module-name, got: {err_msg}"
        );
    }

    #[test]
    fn test_validation_considers_both_cli_and_config() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();

        // Config provides uproject_dir
        let mut config = HashMap::new();
        config.insert(
            "language".to_string(),
            serde_json::Value::String("unrealcpp".to_string()),
        );
        config.insert(
            "uproject_dir".to_string(),
            serde_json::Value::String("/config/path".to_string()),
        );

        // CLI provides module_name
        let matches =
            cmd.clone()
                .get_matches_from(vec!["generate", "--lang", "unrealcpp", "--module-name", "MyModule"]);

        let command_config = CommandConfig::new(&schema, config, &matches).unwrap();

        // Both should be available (one from CLI, one from config)
        let uproject_dir = command_config.get_one::<PathBuf>("uproject_dir").unwrap();
        let module_name = command_config.get_one::<String>("module_name").unwrap();

        assert_eq!(uproject_dir, Some(PathBuf::from("/config/path")));
        assert_eq!(module_name, Some("MyModule".to_string()));
    }
}
