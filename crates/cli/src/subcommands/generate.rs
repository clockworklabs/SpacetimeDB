#![warn(clippy::uninlined_format_args)]

use anyhow::Context;
use clap::parser::ValueSource;
use clap::Arg;
use clap::ArgAction::Set;
use fs_err as fs;
use spacetimedb_codegen::{generate, Csharp, Lang, OutputFile, Rust, TypeScript, UnrealCpp, AUTO_GENERATED_PREFIX};
use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::{sats, RawModuleDef};
use spacetimedb_schema;
use spacetimedb_schema::def::ModuleDef;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::project_config::{CommandConfig, CommandSchema, CommandSchemaBuilder, Key, SpacetimeConfig};
use crate::tasks::csharp::dotnet_format;
use crate::tasks::rust::rustfmt;
use crate::util::{resolve_sibling_binary, y_or_n};
use crate::Config;
use crate::{build, common_args};
use clap::builder::PossibleValue;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::io::Read;

/// Build the CommandSchema for generate command
fn build_generate_schema(command: &clap::Command) -> Result<CommandSchema, anyhow::Error> {
    CommandSchemaBuilder::new()
        .key(Key::new::<Language>("language").from_clap("lang").required())
        .key(Key::new::<PathBuf>("out_dir"))
        .key(Key::new::<PathBuf>("uproject_dir"))
        .key(Key::new::<PathBuf>("module_path").from_clap("project_path"))
        .key(Key::new::<PathBuf>("wasm_file"))
        .key(Key::new::<PathBuf>("js_file"))
        .key(Key::new::<String>("namespace"))
        .key(Key::new::<String>("module_name"))
        .key(Key::new::<String>("build_options"))
        .exclude("json_module")
        .exclude("force")
        .build(command)
        .map_err(Into::into)
}

/// Get filtered generate configs based on CLI arguments
fn get_filtered_generate_configs<'a>(
    spacetime_config: &'a SpacetimeConfig,
    schema: &'a CommandSchema,
    args: &clap::ArgMatches,
) -> Result<Vec<CommandConfig<'a>>, anyhow::Error> {
    // Get all generate configs from spacetime.json
    let all_configs: Vec<HashMap<String, Value>> = spacetime_config.generate.as_ref().cloned().unwrap_or_default();

    // If no config file, return empty (will use CLI args only)
    if all_configs.is_empty() {
        return Ok(vec![]);
    }

    // Build CommandConfig for each generate config
    let all_command_configs: Vec<CommandConfig> = all_configs
        .into_iter()
        .map(|config| {
            let command_config = CommandConfig::new(schema, config)?;
            command_config.validate()?;
            Ok(command_config)
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()?;

    // Filter by module_path if provided via CLI
    let filtered_configs: Vec<CommandConfig> = if schema.is_from_cli(args, "module_path") {
        let cli_module_path = schema.get_clap_arg::<PathBuf>(args, "module_path")?;
        all_command_configs
            .into_iter()
            .filter(|config| {
                let config_module_path = config.get_one::<PathBuf>(args, "module_path").ok().flatten();
                config_module_path.as_ref() == cli_module_path.as_ref()
            })
            .collect()
    } else {
        all_command_configs
    };

    Ok(filtered_configs)
}

pub fn cli() -> clap::Command {
    clap::Command::new("generate")
        .about("Generate client files for a spacetime module.")
        .override_usage("spacetime generate --lang <LANG> --out-dir <DIR> [--project-path <DIR> | --bin-path <PATH> | --module-name <MODULE_NAME> | --uproject-dir <DIR>]")
        .arg(
            Arg::new("wasm_file")
                .value_parser(clap::value_parser!(PathBuf))
                .long("bin-path")
                .short('b')
                .group("source")
                .conflicts_with("project_path")
                .conflicts_with("build_options")
                .help("The system path (absolute or relative) to the compiled wasm binary we should inspect"),
        )
        .arg(
            Arg::new("js_file")
                .value_parser(clap::value_parser!(PathBuf))
                .long("js-path")
                .short('j')
                .group("source")
                .conflicts_with("project_path")
                .conflicts_with("build_options")
                .help("The system path (absolute or relative) to the bundled javascript file we should inspect"),
        )
        .arg(
            Arg::new("project_path")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(".")
                .long("project-path")
                .short('p')
                .group("source")
                .help("The system path (absolute or relative) to the project you would like to inspect"),
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
                .long("foo-bar")
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
        .arg(common_args::yes())
        .after_help("Run `spacetime help publish` for more detailed information.")
}

pub async fn exec(config: Config, args: &clap::ArgMatches) -> anyhow::Result<()> {
    exec_ex(config, args, extract_descriptions).await
}

/// Like `exec`, but lets you specify a custom a function to extract a schema from a file.
pub async fn exec_ex(
    config: Config,
    args: &clap::ArgMatches,
    extract_descriptions: ExtractDescriptions,
) -> anyhow::Result<()> {
    // Build schema
    let cmd = cli();
    let schema = build_generate_schema(&cmd)?;

    // Get generate configs (from spacetime.json or empty)
    let spacetime_config_opt = SpacetimeConfig::find_and_load()?;
    let generate_configs = if let Some((config_path, ref spacetime_config)) = spacetime_config_opt {
        println!("Using configuration from {}", config_path.display());
        get_filtered_generate_configs(spacetime_config, &schema, args)?
    } else {
        vec![CommandConfig::new(&schema, HashMap::new())?]
    };

    // Execute generate for each config
    for command_config in generate_configs {
        // Get values using command_config.get_one() which merges CLI + config
        let project_path = command_config
            .get_one::<PathBuf>(args, "module_path")?
            .unwrap_or_else(|| PathBuf::from("."));
        let wasm_file = command_config.get_one::<PathBuf>(args, "wasm_file")?;
        let js_file = command_config.get_one::<PathBuf>(args, "js_file")?;
        let json_module = args.get_many::<PathBuf>("json_module");
        let lang = command_config
            .get_one::<Language>(args, "language")?
            .ok_or_else(|| anyhow::anyhow!("Language is required (use --lang or add to config)"))?;

        println!(
            "Generating {} module bindings for module {}",
            lang.display_name(),
            project_path.display()
        );

        let namespace = command_config
            .get_one::<String>(args, "namespace")?
            .unwrap_or_else(|| "SpacetimeDB.Types".to_string());
        let module_name = command_config.get_one::<String>(args, "module_name")?;
        let force = args.get_flag("force");
        let build_options = command_config
            .get_one::<String>(args, "build_options")?
            .unwrap_or_else(|| String::new());

        // Validate namespace is only used with csharp
        if args.value_source("namespace") == Some(ValueSource::CommandLine) && lang != Language::Csharp {
            return Err(anyhow::anyhow!("--namespace is only supported with --lang csharp"));
        }

        // Get output directory (either out_dir or uproject_dir)
        let out_dir = command_config
            .get_one::<PathBuf>(args, "out_dir")?
            .or_else(|| command_config.get_one::<PathBuf>(args, "uproject_dir").ok().flatten())
            .ok_or_else(|| anyhow::anyhow!("Either --out-dir or --uproject-dir is required"))?;

        // Validate language-specific requirements
        match lang {
            Language::Rust | Language::Csharp | Language::TypeScript => {
                // These languages require out_dir (not uproject_dir)
                if command_config.get_one::<PathBuf>(args, "out_dir")?.is_none() {
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
                if command_config.get_one::<PathBuf>(args, "uproject_dir")?.is_none() {
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

        for OutputFile { filename, code } in generate(&module, gen_lang) {
            let fname = Path::new(&filename);
            // If a generator asks for a file in a subdirectory, create the subdirectory first.
            if let Some(parent) = fname.parent().filter(|p| !p.as_os_str().is_empty()) {
                fs::create_dir_all(out_dir.join(parent))?;
            }
            let path = out_dir.join(fname);
            if !path.exists() || fs::read_to_string(&path)? != code {
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
fn extract_descriptions(wasm_file: &Path) -> anyhow::Result<ModuleDef> {
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
