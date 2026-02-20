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

use crate::spacetime_config::{
    find_and_load_with_env, CommandConfig, CommandSchema, CommandSchemaBuilder, Key, LoadedConfig, SpacetimeConfig,
};
use crate::tasks::csharp::dotnet_format;
use crate::tasks::rust::rustfmt;
use crate::util::{resolve_sibling_binary, y_or_n};
use crate::Config;
use crate::{build, common_args};
use clap::builder::PossibleValue;

use std::collections::{BTreeSet, HashMap};
use std::io::Read;

/// Build the CommandSchema for generate command configuration.
///
/// This schema is used to validate and merge values from both the config file
/// and CLI arguments, with CLI arguments taking precedence over config values.
fn build_generate_config_schema(command: &clap::Command) -> Result<CommandSchema, anyhow::Error> {
    CommandSchemaBuilder::new()
        .key(
            Key::new("language")
                .from_clap("lang")
                .required()
                .generate_entry_specific(),
        )
        .key(Key::new("out_dir").generate_entry_specific())
        .key(Key::new("uproject_dir").generate_entry_specific())
        .key(Key::new("module_path").module_specific())
        .key(Key::new("wasm_file").module_specific())
        .key(Key::new("js_file").module_specific())
        .key(Key::new("namespace").generate_entry_specific())
        .key(Key::new("unreal_module_name").generate_entry_specific())
        .key(Key::new("build_options").module_specific())
        .key(Key::new("include_private"))
        .exclude("json_module")
        .exclude("force")
        .exclude("no_config")
        .exclude("env")
        .exclude("database")
        .build(command)
        .map_err(Into::into)
}

/// Get filtered generate configs based on CLI arguments.
///
/// Uses the database-centric model: collects all targets with inheritance,
/// filters by database name (glob), then collects generate entries from matched targets.
/// Deduplicates by (canonical_module_path, serialized_generate_entry).
fn get_filtered_generate_configs<'a>(
    spacetime_config: &SpacetimeConfig,
    command: &clap::Command,
    schema: &'a CommandSchema,
    args: &'a clap::ArgMatches,
) -> Result<Vec<CommandConfig<'a>>, anyhow::Error> {
    // Get all database targets from config with parent→child inheritance
    let all_targets = spacetime_config.collect_all_targets_with_inheritance();

    if all_targets.is_empty() {
        return Ok(vec![]);
    }

    // Filter by database name pattern (glob) if provided via CLI
    let filtered_targets = if let Some(cli_database) = args.get_one::<String>("database") {
        let pattern =
            glob::Pattern::new(cli_database).with_context(|| format!("Invalid glob pattern: {cli_database}"))?;

        let matched: Vec<_> = all_targets
            .into_iter()
            .filter(|target| {
                target
                    .fields
                    .get("database")
                    .and_then(|v| v.as_str())
                    .is_some_and(|db| pattern.matches(db))
            })
            .collect();

        if matched.is_empty() {
            anyhow::bail!(
                "No database target matches '{}'. Available databases: {}",
                cli_database,
                spacetime_config
                    .collect_all_targets_with_inheritance()
                    .iter()
                    .filter_map(|t| t.fields.get("database").and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        matched
    } else {
        all_targets
    };

    // Collect generate entries from matched targets, inheriting entity fields
    // Deduplicate by (module_path, serialized_generate_entry)
    let mut seen = std::collections::HashSet::new();
    let mut generate_configs = Vec::new();

    for target in &filtered_targets {
        let generate_entries = match &target.generate {
            Some(entries) if !entries.is_empty() => entries,
            _ => continue,
        };

        // Get module_path from the target's entity fields for dedup
        let module_path = target.fields.get("module-path").and_then(|v| v.as_str()).unwrap_or("");

        for entry in generate_entries {
            // Deduplicate: same module path + same generate entry config = generate once
            let dedup_key = format!("{}:{}", module_path, serde_json::to_string(entry).unwrap_or_default());
            if !seen.insert(dedup_key) {
                continue;
            }

            // Merge entity-level fields (module-path, etc.) with the generate entry
            let mut merged = entry.clone();
            // Inherit module-path from the target entity if not set in the generate entry
            if let Some(mp) = target.fields.get("module-path") {
                merged.entry("module-path".to_string()).or_insert_with(|| mp.clone());
            }

            let command_config = CommandConfig::new(schema, merged, args)?;
            command_config.validate()?;
            generate_configs.push(command_config);
        }
    }

    if generate_configs.is_empty() {
        return Ok(vec![]);
    }

    // Validate generate-entry-specific flags when multiple entries
    schema.validate_no_generate_entry_specific_cli_args(command, args, generate_configs.len())?;

    // Also validate module-specific flags
    schema.validate_no_module_specific_cli_args_for_multiple_targets(
        command,
        args,
        generate_configs.len(),
        "generating for multiple targets",
        "Please specify a database name to select a single target, or remove these arguments.",
    )?;

    Ok(generate_configs)
}

pub fn cli() -> clap::Command {
    clap::Command::new("generate")
        .about("Generate client files for a spacetime module.")
        .override_usage("generate [DATABASE] --lang <LANG> --out-dir <DIR> [--module-path <DIR> | --bin-path <PATH> | --unreal-module-name <MODULE_NAME> | --uproject-dir <DIR> | --include-private]")
        .arg(
            Arg::new("database")
                .help("Database name or glob pattern to filter which databases to generate for"),
        )
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
            Arg::new("unreal_module_name")
                .long("unreal-module-name")
                .alias("module-name")
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
        .arg(
            Arg::new("no_config")
                .long("no-config")
                .action(SetTrue)
                .help("Ignore spacetime.json configuration")
        )
        .arg(
            Arg::new("env")
                .long("env")
                .value_name("ENV")
                .action(Set)
                .help("Environment name for config file layering (e.g., dev, staging)")
        )
        .after_help("Run `spacetime help generate` for more detailed information.")
}

pub async fn exec(config: Config, args: &clap::ArgMatches) -> anyhow::Result<()> {
    exec_ex(config, args, extract_descriptions, false, None).await
}

#[derive(Debug, Clone)]
pub struct GenerateRunConfig {
    pub project_path: PathBuf,
    pub wasm_file: Option<PathBuf>,
    pub js_file: Option<PathBuf>,
    pub lang: Language,
    pub namespace: String,
    pub module_name: Option<String>,
    pub build_options: String,
    pub out_dir: PathBuf,
    pub include_private: bool,
}

fn prepare_generate_run_configs<'a>(
    generate_configs: Vec<CommandConfig<'a>>,
    _using_config: bool,
    config_dir: Option<&Path>,
) -> anyhow::Result<Vec<GenerateRunConfig>> {
    let mut runs = Vec::with_capacity(generate_configs.len());

    for command_config in generate_configs {
        let project_path = command_config
            .get_resolved_path("module_path", config_dir)?
            .unwrap_or_else(|| {
                config_dir
                    .map(|d| d.join("spacetimedb"))
                    .unwrap_or_else(|| PathBuf::from("spacetimedb"))
            });

        let wasm_file = command_config.get_one::<PathBuf>("wasm_file")?;
        let js_file = command_config.get_one::<PathBuf>("js_file")?;
        let requested_lang = command_config.get_one::<Language>("language")?;
        let namespace = command_config
            .get_one::<String>("namespace")?
            .unwrap_or_else(|| "SpacetimeDB.Types".to_string());
        let module_name = command_config.get_one::<String>("unreal_module_name")?;
        let build_options = command_config
            .get_one::<String>("build_options")?
            .unwrap_or_else(String::new);

        // Validate Unreal-specific args first to preserve focused errors for this mode.
        if requested_lang == Some(Language::UnrealCpp) {
            if command_config.get_one::<PathBuf>("uproject_dir")?.is_none() {
                return Err(anyhow::anyhow!("--uproject-dir is required for --lang unrealcpp"));
            }
            if module_name.is_none() {
                return Err(anyhow::anyhow!("--unreal-module-name is required for --lang unrealcpp"));
            }
        }

        // Validate module source path for source-based generation.
        if wasm_file.is_none() && js_file.is_none() && !project_path.is_dir() {
            anyhow::bail!(
                "Could not find module source at '{}'. \
                 If this is not correct, pass --module-path or add module-path in spacetime.json. \
                 You can also pass --bin-path/--js-path to generate without building from source.",
                project_path.display()
            );
        }

        let lang = match requested_lang {
            Some(lang) => lang,
            None => {
                let client_project_dir = config_dir.unwrap_or_else(|| Path::new("."));
                let detected = detect_default_language(client_project_dir)?;
                println!(
                    "Detected client language '{}' from '{}'. If this is not correct, pass --lang or add a generate target in spacetime.json.",
                    language_cli_name(detected),
                    client_project_dir.display()
                );
                detected
            }
        };

        let out_dir = command_config
            .get_resolved_path("out_dir", config_dir)?
            .or_else(|| {
                command_config
                    .get_resolved_path("uproject_dir", config_dir)
                    .ok()
                    .flatten()
            })
            .or_else(|| default_out_dir_for_language(lang).map(|p| config_dir.map(|d| d.join(&p)).unwrap_or(p)))
            .ok_or_else(|| anyhow::anyhow!("Either --out-dir or --uproject-dir is required"))?;

        let include_private = command_config.get_one::<bool>("include_private")?.unwrap_or(false);

        runs.push(GenerateRunConfig {
            project_path,
            wasm_file,
            js_file,
            lang,
            namespace,
            module_name,
            build_options,
            out_dir,
            include_private,
        });
    }

    Ok(runs)
}

fn detect_default_language(client_project_dir: &Path) -> anyhow::Result<Language> {
    if client_project_dir.join("package.json").exists() {
        return Ok(Language::TypeScript);
    }
    if client_project_dir.join("Cargo.toml").exists() {
        return Ok(Language::Rust);
    }
    if let Ok(entries) = fs::read_dir(client_project_dir) {
        if entries
            .flatten()
            .any(|entry| entry.path().extension().is_some_and(|e| e == "csproj"))
        {
            return Ok(Language::Csharp);
        }
    }

    anyhow::bail!(
        "Could not auto-detect client language from '{}'. \
         If this is not correct, pass --lang or add a generate target in spacetime.json.",
        client_project_dir.display()
    );
}

fn language_cli_name(lang: Language) -> &'static str {
    match lang {
        Language::Rust => "rust",
        Language::Csharp => "csharp",
        Language::TypeScript => "typescript",
        Language::UnrealCpp => "unrealcpp",
    }
}

pub fn default_out_dir_for_language(lang: Language) -> Option<PathBuf> {
    match lang {
        Language::Rust | Language::TypeScript => Some(PathBuf::from("src/module_bindings")),
        Language::Csharp => Some(PathBuf::from("module_bindings")),
        Language::UnrealCpp => None,
    }
}

pub fn resolve_language(module_path: &Path, requested: Option<Language>) -> anyhow::Result<Language> {
    match requested {
        Some(lang) => Ok(lang),
        None => detect_default_language(module_path),
    }
}

pub fn build_generate_entry(
    module_path: Option<&Path>,
    language: Option<Language>,
    out_dir: Option<&Path>,
) -> HashMap<String, serde_json::Value> {
    let mut entry = HashMap::new();
    if let Some(lang) = language {
        entry.insert("language".to_string(), serde_json::json!(language_cli_name(lang)));
    }
    if let Some(path) = module_path {
        entry.insert("module-path".to_string(), serde_json::json!(path));
    }
    if let Some(path) = out_dir {
        entry.insert("out-dir".to_string(), serde_json::json!(path));
    }
    entry
}

pub async fn run_prepared_generate_configs(
    run_configs: Vec<GenerateRunConfig>,
    extract_descriptions: ExtractDescriptions,
    json_module: Option<Vec<PathBuf>>,
    force: bool,
    namespace_from_cli: bool,
) -> anyhow::Result<()> {
    for run in run_configs {
        println!(
            "Generating {} module bindings for module {}",
            run.lang.display_name(),
            run.project_path.display()
        );

        if namespace_from_cli && run.lang != Language::Csharp {
            return Err(anyhow::anyhow!("--namespace is only supported with --lang csharp"));
        }

        let module: ModuleDef = if let Some(paths) = &json_module {
            let DeserializeWrapper::<RawModuleDef>(module) = if let Some(path) = paths.first() {
                serde_json::from_slice(&fs::read(path)?)?
            } else {
                serde_json::from_reader(std::io::stdin().lock())?
            };
            module.try_into()?
        } else {
            let path = if let Some(path) = &run.wasm_file {
                println!("Skipping build. Instead we are inspecting {}", path.display());
                path.clone()
            } else if let Some(path) = &run.js_file {
                println!("Skipping build. Instead we are inspecting {}", path.display());
                path.clone()
            } else {
                let (path, _) = build::exec_with_argstring(&run.project_path, &run.build_options).await?;
                path
            };
            let spinner = indicatif::ProgressBar::new_spinner();
            spinner.enable_steady_tick(std::time::Duration::from_millis(60));
            spinner.set_message(format!("Extracting schema from {}...", path.display()));
            extract_descriptions(&path).context("could not extract schema")?
        };

        fs::create_dir_all(&run.out_dir)?;

        let mut paths = BTreeSet::new();
        let private_tables = private_table_names(&module);
        if !private_tables.is_empty() && !run.include_private {
            println!("Skipping private tables during codegen: {}.", private_tables.join(", "));
        }
        let mut options = CodegenOptions::default();
        if run.include_private {
            options.visibility = CodegenVisibility::IncludePrivate;
        }

        let csharp_lang;
        let unreal_cpp_lang;
        let gen_lang = match run.lang {
            Language::Csharp => {
                csharp_lang = Csharp {
                    namespace: &run.namespace,
                };
                &csharp_lang as &dyn Lang
            }
            Language::UnrealCpp => {
                unreal_cpp_lang = UnrealCpp {
                    module_name: run.module_name.as_ref().unwrap(),
                    uproject_dir: &run.out_dir,
                };
                &unreal_cpp_lang as &dyn Lang
            }
            Language::Rust => &Rust,
            Language::TypeScript => &TypeScript,
        };

        for OutputFile { filename, code } in generate(&module, gen_lang, &options) {
            let fname = Path::new(&filename);
            if let Some(parent) = fname.parent().filter(|p| !p.as_os_str().is_empty()) {
                println!("Creating directory {}", run.out_dir.join(parent).display());
                fs::create_dir_all(run.out_dir.join(parent))?;
            }
            let path = run.out_dir.join(fname);
            if !path.exists() || fs::read_to_string(&path)? != code {
                println!("Writing file {}", path.display());
                fs::write(&path, code)?;
            }
            paths.insert(path);
        }

        let cleanup_root = match run.lang {
            Language::UnrealCpp => run.out_dir.join("Source").join(run.module_name.as_ref().unwrap()),
            _ => run.out_dir.clone(),
        };

        let mut auto_generated_buf: [u8; AUTO_GENERATED_PREFIX.len()] = [0; AUTO_GENERATED_PREFIX.len()];
        let files_to_delete = walkdir::WalkDir::new(&cleanup_root)
            .into_iter()
            .map(|entry_result| {
                let entry = entry_result?;
                if !entry.file_type().is_file() {
                    return Ok(None);
                }
                let path = entry.into_path();
                if paths.contains(&path) {
                    return Ok(None);
                }
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

        if let Err(err) = run.lang.format_files(&run.out_dir, paths) {
            eprintln!("Could not format generated files: {err}");
        }

        println!("Generate finished successfully.");
    }

    Ok(())
}

/// Like `exec`, but lets you specify a custom a function to extract a schema from a file.
pub async fn exec_ex(
    _config: Config,
    args: &clap::ArgMatches,
    extract_descriptions: ExtractDescriptions,
    quiet_config: bool,
    pre_loaded_config: Option<&LoadedConfig>,
) -> anyhow::Result<()> {
    // Build schema
    let cmd = cli();
    let schema = build_generate_config_schema(&cmd)?;

    let no_config = args.get_flag("no_config");
    let env = args.get_one::<String>("env").map(|s| s.as_str());

    // Get generate configs (from spacetime.json or empty)
    let owned_loaded;
    let loaded_config_ref = if no_config {
        None
    } else if let Some(pre) = pre_loaded_config {
        Some(pre)
    } else {
        owned_loaded = find_and_load_with_env(env)?;
        owned_loaded.as_ref().inspect(|loaded| {
            if !quiet_config {
                for path in &loaded.loaded_files {
                    println!("Using configuration from {}", path.display());
                }
            }
        })
    };
    let (using_config, generate_configs) = if let Some(loaded) = loaded_config_ref {
        let filtered = get_filtered_generate_configs(&loaded.config, &cmd, &schema, args)?;
        if filtered.is_empty() {
            (false, vec![CommandConfig::new(&schema, HashMap::new(), args)?])
        } else {
            (true, filtered)
        }
    } else {
        (false, vec![CommandConfig::new(&schema, HashMap::new(), args)?])
    };

    let config_dir = loaded_config_ref.map(|lc| lc.config_dir.as_path());
    let run_configs = prepare_generate_run_configs(generate_configs, using_config, config_dir)?;
    let json_module = args
        .get_many::<PathBuf>("json_module")
        .map(|vals| vals.cloned().collect::<Vec<_>>());
    let force = args.get_flag("force");
    let namespace_from_cli = args.value_source("namespace") == Some(ValueSource::CommandLine);

    run_prepared_generate_configs(
        run_configs,
        extract_descriptions,
        json_module,
        force,
        namespace_from_cli,
    )
    .await
}

/// Execute generate from explicit entries without parsing generate CLI arguments
/// or loading any config files from disk.
pub async fn exec_from_entries(
    entries: Vec<HashMap<String, serde_json::Value>>,
    extract_descriptions: ExtractDescriptions,
    force: bool,
    config_dir: Option<&Path>,
) -> anyhow::Result<()> {
    let cmd = cli();
    let schema = build_generate_config_schema(&cmd)?;
    let empty_matches = cmd.get_matches_from(vec!["generate"]);

    let generate_configs: Vec<CommandConfig> = entries
        .into_iter()
        .map(|entry| {
            let command_config = CommandConfig::new(&schema, entry, &empty_matches)?;
            command_config.validate()?;
            Ok(command_config)
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()?;

    let run_configs = prepare_generate_run_configs(generate_configs, true, config_dir)?;
    run_prepared_generate_configs(run_configs, extract_descriptions, None, force, false).await
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Csharp,
    TypeScript,
    Rust,
    #[serde(alias = "uecpp", alias = "ue5cpp", alias = "unreal")]
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

    /// Helper to build a SpacetimeConfig with generate entries (database-centric)
    fn make_gen_config(
        fields: HashMap<String, serde_json::Value>,
        generate: Vec<HashMap<String, serde_json::Value>>,
    ) -> SpacetimeConfig {
        SpacetimeConfig {
            generate: Some(generate),
            additional_fields: fields,
            ..Default::default()
        }
    }

    #[test]
    fn test_filter_by_database_name() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();

        let mut db1_fields = HashMap::new();
        db1_fields.insert("database".to_string(), serde_json::json!("db1"));
        db1_fields.insert("module-path".to_string(), serde_json::json!("./module1"));

        let gen1 = {
            let mut m = HashMap::new();
            m.insert("language".to_string(), serde_json::json!("rust"));
            m.insert("out_dir".to_string(), serde_json::json!("/tmp/out1"));
            m
        };

        let mut db2_fields = HashMap::new();
        db2_fields.insert("database".to_string(), serde_json::json!("db2"));
        db2_fields.insert("module-path".to_string(), serde_json::json!("./module2"));

        let gen2 = {
            let mut m = HashMap::new();
            m.insert("language".to_string(), serde_json::json!("typescript"));
            m.insert("out_dir".to_string(), serde_json::json!("/tmp/out2"));
            m
        };

        let spacetime_config = SpacetimeConfig {
            children: Some(vec![
                make_gen_config(db1_fields, vec![gen1]),
                make_gen_config(db2_fields, vec![gen2]),
            ]),
            ..Default::default()
        };

        // Filter by db1
        let matches = cmd.clone().get_matches_from(vec!["generate", "db1"]);
        let filtered = get_filtered_generate_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        assert_eq!(filtered.len(), 1, "Should only match db1's generate entry");
    }

    #[test]
    fn test_no_filter_returns_all_generate_entries() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();

        let mut fields = HashMap::new();
        fields.insert("module-path".to_string(), serde_json::json!("./module"));

        let gen1 = {
            let mut m = HashMap::new();
            m.insert("language".to_string(), serde_json::json!("rust"));
            m.insert("out_dir".to_string(), serde_json::json!("/tmp/out1"));
            m
        };
        let gen2 = {
            let mut m = HashMap::new();
            m.insert("language".to_string(), serde_json::json!("typescript"));
            m.insert("out_dir".to_string(), serde_json::json!("/tmp/out2"));
            m
        };

        let spacetime_config = make_gen_config(fields, vec![gen1, gen2]);

        let matches = cmd.clone().get_matches_from(vec!["generate"]);
        let filtered = get_filtered_generate_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_generate_entry_inherits_module_path_from_parent() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();

        let mut parent_fields = HashMap::new();
        parent_fields.insert("module-path".to_string(), serde_json::json!("./server"));

        let mut child_fields = HashMap::new();
        child_fields.insert("database".to_string(), serde_json::json!("my-db"));

        let gen = {
            let mut m = HashMap::new();
            m.insert("language".to_string(), serde_json::json!("rust"));
            m.insert("out_dir".to_string(), serde_json::json!("/tmp/out"));
            m
        };

        let spacetime_config = SpacetimeConfig {
            additional_fields: parent_fields,
            children: Some(vec![make_gen_config(child_fields, vec![gen])]),
            ..Default::default()
        };

        let matches = cmd.clone().get_matches_from(vec!["generate", "my-db"]);
        let filtered = get_filtered_generate_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        assert_eq!(filtered.len(), 1);
        // module_path should be inherited from parent
        assert_eq!(
            filtered[0].get_one::<PathBuf>("module_path").unwrap(),
            Some(PathBuf::from("./server"))
        );
    }

    #[test]
    fn test_generate_deduplication() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();

        let gen = {
            let mut m = HashMap::new();
            m.insert("language".to_string(), serde_json::json!("typescript"));
            m.insert("out_dir".to_string(), serde_json::json!("/tmp/out"));
            m
        };

        let mut parent_fields = HashMap::new();
        parent_fields.insert("module-path".to_string(), serde_json::json!("./server"));

        let spacetime_config = SpacetimeConfig {
            additional_fields: parent_fields,
            generate: Some(vec![gen]),
            children: Some(vec![
                {
                    let mut f = HashMap::new();
                    f.insert("database".to_string(), serde_json::json!("region-1"));
                    SpacetimeConfig {
                        additional_fields: f,
                        ..Default::default()
                    }
                },
                {
                    let mut f = HashMap::new();
                    f.insert("database".to_string(), serde_json::json!("region-2"));
                    SpacetimeConfig {
                        additional_fields: f,
                        ..Default::default()
                    }
                },
            ]),
            ..Default::default()
        };

        let matches = cmd.clone().get_matches_from(vec!["generate"]);
        let filtered = get_filtered_generate_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        // Same module-path + same generate entry = deduplicated
        assert_eq!(
            filtered.len(),
            1,
            "Expected deduplication: same module + same generate config = generate once"
        );
    }

    #[test]
    fn test_generate_entry_specific_args_error_with_multiple_entries() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();

        let mut fields = HashMap::new();
        fields.insert("module-path".to_string(), serde_json::json!("./module"));

        let gen1 = {
            let mut m = HashMap::new();
            m.insert("language".to_string(), serde_json::json!("rust"));
            m.insert("out_dir".to_string(), serde_json::json!("/tmp/out1"));
            m
        };
        let gen2 = {
            let mut m = HashMap::new();
            m.insert("language".to_string(), serde_json::json!("typescript"));
            m.insert("out_dir".to_string(), serde_json::json!("/tmp/out2"));
            m
        };

        let spacetime_config = make_gen_config(fields, vec![gen1, gen2]);

        let matches = cmd
            .clone()
            .get_matches_from(vec!["generate", "--out-dir", "/tmp/override"]);
        let err = get_filtered_generate_configs(&spacetime_config, &cmd, &schema, &matches).unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("--out-dir"),
            "Expected error to mention --out-dir, got: {err_msg}"
        );
    }

    // Language-Specific Validation Tests

    #[test]
    fn test_rust_defaults_out_dir() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();
        let matches = cmd.clone().get_matches_from(vec!["generate", "--lang", "rust"]);

        let temp = tempfile::TempDir::new().unwrap();
        let module_dir = temp.path().join("spacetimedb");
        std::fs::create_dir_all(&module_dir).unwrap();
        std::fs::write(
            module_dir.join("Cargo.toml"),
            "[package]\nname = \"m\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let mut cfg = HashMap::new();
        cfg.insert(
            "module-path".to_string(),
            serde_json::Value::String(module_dir.display().to_string()),
        );

        let command_config = CommandConfig::new(&schema, cfg, &matches).unwrap();
        let runs = prepare_generate_run_configs(vec![command_config], false, None).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].out_dir, PathBuf::from("src/module_bindings"));
    }

    #[test]
    fn test_module_path_defaults_to_spacetimedb() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();
        let matches = cmd
            .clone()
            .get_matches_from(vec!["generate", "--lang", "rust", "--bin-path", "dummy.wasm"]);
        let command_config = CommandConfig::new(&schema, HashMap::new(), &matches).unwrap();
        let runs = prepare_generate_run_configs(vec![command_config], false, None).unwrap();
        assert_eq!(runs[0].project_path, PathBuf::from("spacetimedb"));
    }

    #[test]
    fn test_defaults_resolve_relative_to_config_dir_when_config_exists() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();
        let matches = cmd.clone().get_matches_from(vec!["generate", "--lang", "rust"]);

        let config_dir = tempfile::TempDir::new().unwrap();
        let module_dir = config_dir.path().join("spacetimedb");
        std::fs::create_dir_all(&module_dir).unwrap();
        std::fs::write(
            module_dir.join("Cargo.toml"),
            "[package]\nname = \"m\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let command_config = CommandConfig::new(&schema, HashMap::new(), &matches).unwrap();
        let runs = prepare_generate_run_configs(vec![command_config], true, Some(config_dir.path())).unwrap();

        assert_eq!(runs[0].project_path, module_dir);
        assert_eq!(runs[0].out_dir, config_dir.path().join("src/module_bindings"));
    }

    #[test]
    fn test_cli_relative_out_dir_is_not_rebased_to_config_dir() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();
        let config_dir = tempfile::TempDir::new().unwrap();
        let module_dir = config_dir.path().join("spacetimedb");
        std::fs::create_dir_all(&module_dir).unwrap();
        std::fs::write(
            module_dir.join("Cargo.toml"),
            "[package]\nname = \"m\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let matches =
            cmd.clone()
                .get_matches_from(vec!["generate", "--lang", "rust", "--out-dir", "src/module_bindings"]);
        let mut cfg = HashMap::new();
        cfg.insert(
            "module-path".to_string(),
            serde_json::Value::String(module_dir.display().to_string()),
        );

        let command_config = CommandConfig::new(&schema, cfg, &matches).unwrap();
        let runs = prepare_generate_run_configs(vec![command_config], true, Some(config_dir.path())).unwrap();

        assert_eq!(runs[0].out_dir, PathBuf::from("src/module_bindings"));
    }

    #[test]
    fn test_typescript_defaults_out_dir() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();
        let matches = cmd.clone().get_matches_from(vec!["generate", "--lang", "typescript"]);
        let temp = tempfile::TempDir::new().unwrap();
        let module_dir = temp.path().join("spacetimedb");
        std::fs::create_dir_all(&module_dir).unwrap();
        let mut cfg = HashMap::new();
        cfg.insert(
            "module-path".to_string(),
            serde_json::Value::String(module_dir.display().to_string()),
        );
        let command_config = CommandConfig::new(&schema, cfg, &matches).unwrap();
        let runs = prepare_generate_run_configs(vec![command_config], false, None).unwrap();
        assert_eq!(runs[0].out_dir, PathBuf::from("src/module_bindings"));
    }

    #[test]
    fn test_csharp_defaults_out_dir() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();
        let matches = cmd.clone().get_matches_from(vec!["generate", "--lang", "csharp"]);
        let temp = tempfile::TempDir::new().unwrap();
        let module_dir = temp.path().join("spacetimedb");
        std::fs::create_dir_all(&module_dir).unwrap();
        let mut cfg = HashMap::new();
        cfg.insert(
            "module-path".to_string(),
            serde_json::Value::String(module_dir.display().to_string()),
        );
        let command_config = CommandConfig::new(&schema, cfg, &matches).unwrap();
        let runs = prepare_generate_run_configs(vec![command_config], false, None).unwrap();
        assert_eq!(runs[0].out_dir, PathBuf::from("module_bindings"));
    }

    #[test]
    fn test_detect_typescript_language_from_client_project() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();
        let matches = cmd.clone().get_matches_from(vec!["generate"]);
        let temp = tempfile::TempDir::new().unwrap();
        let module_dir = temp.path().join("spacetimedb");
        std::fs::create_dir_all(&module_dir).unwrap();
        std::fs::write(temp.path().join("package.json"), "{\"name\":\"client\"}").unwrap();
        let mut cfg = HashMap::new();
        cfg.insert(
            "module-path".to_string(),
            serde_json::Value::String(module_dir.display().to_string()),
        );
        let command_config = CommandConfig::new(&schema, cfg, &matches).unwrap();
        let runs = prepare_generate_run_configs(vec![command_config], true, Some(temp.path())).unwrap();
        assert_eq!(runs[0].lang, Language::TypeScript);
        assert_eq!(runs[0].out_dir, temp.path().join("src/module_bindings"));
    }

    #[test]
    fn test_detect_csharp_language_from_client_project() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();
        let matches = cmd.clone().get_matches_from(vec!["generate"]);
        let temp = tempfile::TempDir::new().unwrap();
        let module_dir = temp.path().join("spacetimedb");
        std::fs::create_dir_all(&module_dir).unwrap();
        std::fs::write(temp.path().join("Client.csproj"), "<Project/>").unwrap();
        let mut cfg = HashMap::new();
        cfg.insert(
            "module-path".to_string(),
            serde_json::Value::String(module_dir.display().to_string()),
        );
        let command_config = CommandConfig::new(&schema, cfg, &matches).unwrap();
        let runs = prepare_generate_run_configs(vec![command_config], true, Some(temp.path())).unwrap();
        assert_eq!(runs[0].lang, Language::Csharp);
        assert_eq!(runs[0].out_dir, temp.path().join("module_bindings"));
    }

    #[test]
    fn test_error_when_default_module_path_missing_and_lang_not_set() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();
        let matches = cmd.clone().get_matches_from(vec!["generate"]);
        let command_config = CommandConfig::new(&schema, HashMap::new(), &matches).unwrap();
        let err = prepare_generate_run_configs(vec![command_config], false, None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Could not find module source at 'spacetimedb'"));
        assert!(msg.contains("--module-path"));
        assert!(msg.contains("spacetime.json"));
    }

    #[test]
    fn test_error_when_client_language_cannot_be_detected() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();
        let matches = cmd.clone().get_matches_from(vec!["generate"]);
        let temp = tempfile::TempDir::new().unwrap();
        let module_dir = temp.path().join("spacetimedb");
        std::fs::create_dir_all(&module_dir).unwrap();
        let mut cfg = HashMap::new();
        cfg.insert(
            "module-path".to_string(),
            serde_json::Value::String(module_dir.display().to_string()),
        );
        let command_config = CommandConfig::new(&schema, cfg, &matches).unwrap();
        let err = prepare_generate_run_configs(vec![command_config], true, Some(temp.path())).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Could not auto-detect client language"));
        assert!(msg.contains("--lang"));
        assert!(msg.contains("spacetime.json"));
    }

    #[tokio::test]
    async fn test_unrealcpp_requires_uproject_dir_and_unreal_module_name() {
        use crate::config::Config;
        use spacetimedb_paths::cli::CliTomlPath;
        use spacetimedb_paths::FromPathUnchecked;

        let cmd = cli();
        let config = Config::new_with_localhost(CliTomlPath::from_path_unchecked("/tmp/test-config.toml"));

        // Test missing --uproject-dir (use alias --module-name for backwards compat)
        let matches =
            cmd.clone()
                .get_matches_from(vec!["generate", "--lang", "unrealcpp", "--module-name", "MyModule"]);
        let result = exec(config.clone(), &matches).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("--uproject-dir") || err_msg.contains("--out-dir"),
            "Expected error about missing --uproject-dir or --out-dir, got: {err_msg}",
        );

        // Test missing --unreal-module-name
        let matches =
            cmd.clone()
                .get_matches_from(vec!["generate", "--lang", "unrealcpp", "--uproject-dir", "/tmp/out"]);
        let result = exec(config, &matches).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("--unreal-module-name is required for --lang unrealcpp"),
            "Expected error about missing --unreal-module-name, got: {err_msg}"
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

        // CLI provides unreal_module_name (via alias --module-name)
        let matches =
            cmd.clone()
                .get_matches_from(vec!["generate", "--lang", "unrealcpp", "--module-name", "MyModule"]);

        let command_config = CommandConfig::new(&schema, config, &matches).unwrap();

        let uproject_dir = command_config.get_one::<PathBuf>("uproject_dir").unwrap();
        let module_name = command_config.get_one::<String>("unreal_module_name").unwrap();

        assert_eq!(uproject_dir, Some(PathBuf::from("/config/path")));
        assert_eq!(module_name, Some("MyModule".to_string()));
    }

    #[test]
    fn test_generate_dedup_with_inherited_generate_from_parent() {
        // Two sibling databases inheriting the same generate + same module-path from parent
        // should deduplicate to a single generate entry
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();

        let gen = {
            let mut m = HashMap::new();
            m.insert("language".to_string(), serde_json::json!("typescript"));
            m.insert("out_dir".to_string(), serde_json::json!("/tmp/bindings"));
            m
        };

        let mut parent_fields = HashMap::new();
        parent_fields.insert("module-path".to_string(), serde_json::json!("./server"));

        let spacetime_config = SpacetimeConfig {
            additional_fields: parent_fields,
            generate: Some(vec![gen]),
            children: Some(vec![
                {
                    let mut f = HashMap::new();
                    f.insert("database".to_string(), serde_json::json!("region-1"));
                    SpacetimeConfig {
                        additional_fields: f,
                        ..Default::default()
                    }
                },
                {
                    let mut f = HashMap::new();
                    f.insert("database".to_string(), serde_json::json!("region-2"));
                    SpacetimeConfig {
                        additional_fields: f,
                        ..Default::default()
                    }
                },
            ]),
            ..Default::default()
        };

        // No filter — all 3 targets (parent + 2 children) share same module-path + same generate
        let matches = cmd.clone().get_matches_from(vec!["generate"]);
        let filtered = get_filtered_generate_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        // Should be deduplicated to 1 entry since all have same (module_path, generate_entry)
        assert_eq!(
            filtered.len(),
            1,
            "Inherited generate entries with same module-path should be deduplicated"
        );
    }

    #[test]
    fn test_generate_glob_filter_matches_pattern() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();

        let gen = {
            let mut m = HashMap::new();
            m.insert("language".to_string(), serde_json::json!("rust"));
            m.insert("out_dir".to_string(), serde_json::json!("/tmp/out"));
            m
        };

        let spacetime_config = SpacetimeConfig {
            children: Some(vec![
                make_gen_config(
                    {
                        let mut m = HashMap::new();
                        m.insert("database".to_string(), serde_json::json!("region-1"));
                        m.insert("module-path".to_string(), serde_json::json!("./m1"));
                        m
                    },
                    vec![gen.clone()],
                ),
                make_gen_config(
                    {
                        let mut m = HashMap::new();
                        m.insert("database".to_string(), serde_json::json!("region-2"));
                        m.insert("module-path".to_string(), serde_json::json!("./m2"));
                        m
                    },
                    vec![gen.clone()],
                ),
                make_gen_config(
                    {
                        let mut m = HashMap::new();
                        m.insert("database".to_string(), serde_json::json!("global"));
                        m.insert("module-path".to_string(), serde_json::json!("./m3"));
                        m
                    },
                    vec![gen],
                ),
            ]),
            ..Default::default()
        };

        // Glob: region-* should match region-1 and region-2 but not global
        let matches = cmd.clone().get_matches_from(vec!["generate", "region-*"]);
        let filtered = get_filtered_generate_configs(&spacetime_config, &cmd, &schema, &matches).unwrap();

        // region-1 and region-2 have different module-paths, so no dedup → 2 entries
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_generate_error_when_glob_matches_nothing() {
        let cmd = cli();
        let schema = build_generate_config_schema(&cmd).unwrap();

        let gen = {
            let mut m = HashMap::new();
            m.insert("language".to_string(), serde_json::json!("rust"));
            m.insert("out_dir".to_string(), serde_json::json!("/tmp/out"));
            m
        };

        let spacetime_config = make_gen_config(
            {
                let mut m = HashMap::new();
                m.insert("database".to_string(), serde_json::json!("my-db"));
                m.insert("module-path".to_string(), serde_json::json!("./server"));
                m
            },
            vec![gen],
        );

        let matches = cmd.clone().get_matches_from(vec!["generate", "nonexistent-*"]);
        let result = get_filtered_generate_configs(&spacetime_config, &cmd, &schema, &matches);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("No database target matches"),
            "Error should mention no match, got: {err_msg}"
        );
    }

    #[test]
    fn test_language_serde_deserialize_all_variants() {
        // Verify all Language variants deserialize correctly from config JSON strings.
        // This catches drift between the serde and clap ValueEnum impls.
        assert_eq!(
            serde_json::from_value::<Language>(serde_json::Value::String("csharp".into())).unwrap(),
            Language::Csharp
        );
        assert_eq!(
            serde_json::from_value::<Language>(serde_json::Value::String("typescript".into())).unwrap(),
            Language::TypeScript
        );
        assert_eq!(
            serde_json::from_value::<Language>(serde_json::Value::String("rust".into())).unwrap(),
            Language::Rust
        );
        assert_eq!(
            serde_json::from_value::<Language>(serde_json::Value::String("unrealcpp".into())).unwrap(),
            Language::UnrealCpp
        );

        // Aliases
        assert_eq!(
            serde_json::from_value::<Language>(serde_json::Value::String("uecpp".into())).unwrap(),
            Language::UnrealCpp
        );
        assert_eq!(
            serde_json::from_value::<Language>(serde_json::Value::String("ue5cpp".into())).unwrap(),
            Language::UnrealCpp
        );
        assert_eq!(
            serde_json::from_value::<Language>(serde_json::Value::String("unreal".into())).unwrap(),
            Language::UnrealCpp
        );

        // Invalid language should error
        assert!(serde_json::from_value::<Language>(serde_json::Value::String("java".into())).is_err());
    }
}
