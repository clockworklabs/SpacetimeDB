#![warn(clippy::uninlined_format_args)]

use anyhow::Context;
use clap::parser::ValueSource;
use clap::Arg;
use clap::ArgAction::Set;
use fs_err as fs;
use spacetimedb_codegen::{generate, Csharp, Go, Lang, Rust, TypeScript, AUTO_GENERATED_PREFIX};
use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::{sats, RawModuleDef};
use spacetimedb_schema;
use spacetimedb_schema::def::ModuleDef;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::tasks::csharp::dotnet_format;
use crate::tasks::go::go_format;
use crate::tasks::rust::rustfmt;
use crate::util::{resolve_sibling_binary, y_or_n};
use crate::Config;
use crate::{build, common_args};
use clap::builder::PossibleValue;
use std::collections::BTreeSet;
use std::io::Read;

pub fn cli() -> clap::Command {
    clap::Command::new("generate")
        .about("Generate client files for a spacetime module.")
        .override_usage("spacetime generate --lang <LANG> --out-dir <DIR> [--project-path <DIR> | --bin-path <PATH>]")
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
                .required(true)
                .long("out-dir")
                .short('o')
                .help("The system path (absolute or relative) to the generate output directory"),
        )
        .arg(
            Arg::new("namespace")
                .default_value("SpacetimeDB.Types")
                .long("namespace")
                .help("The namespace that should be used"),
        )
        .arg(
            Arg::new("lang")
                .required(true)
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
    let project_path = args.get_one::<PathBuf>("project_path").unwrap();
    let wasm_file = args.get_one::<PathBuf>("wasm_file").cloned();
    let json_module = args.get_many::<PathBuf>("json_module");
    let out_dir = args.get_one::<PathBuf>("out_dir").unwrap();
    let lang = *args.get_one::<Language>("lang").unwrap();
    let namespace = args.get_one::<String>("namespace").unwrap();
    let force = args.get_flag("force");
    let build_options = args.get_one::<String>("build_options").unwrap();

    if args.value_source("namespace") == Some(ValueSource::CommandLine) && lang != Language::Csharp {
        return Err(anyhow::anyhow!("--namespace is only supported with --lang csharp"));
    }

    let module: ModuleDef = if let Some(mut json_module) = json_module {
        let DeserializeWrapper::<RawModuleDef>(module) = if let Some(path) = json_module.next() {
            serde_json::from_slice(&fs::read(path)?)?
        } else {
            serde_json::from_reader(std::io::stdin().lock())?
        };
        module.try_into()?
    } else {
        let wasm_path = if let Some(path) = wasm_file {
            println!("Skipping build. Instead we are inspecting {}", path.display());
            path.clone()
        } else {
            build::exec_with_argstring(config.clone(), project_path, build_options).await?
        };
        let spinner = indicatif::ProgressBar::new_spinner();
        spinner.enable_steady_tick(std::time::Duration::from_millis(60));
        spinner.set_message("Extracting schema from wasm...");
        extract_descriptions(&wasm_path).context("could not extract schema")?
    };

    fs::create_dir_all(out_dir)?;

    let mut paths = BTreeSet::new();

    let csharp_lang;
    let go_lang;
    let gen_lang = match lang {
        Language::Csharp => {
            csharp_lang = Csharp { namespace };
            &csharp_lang as &dyn Lang
        }
        Language::Go => {
            go_lang = Go::default();
            &go_lang as &dyn Lang
        }
        Language::Rust => &Rust,
        Language::TypeScript => &TypeScript,
    };

    for (fname, code) in generate(&module, gen_lang) {
        let fname = Path::new(&fname);
        // If a generator asks for a file in a subdirectory, create the subdirectory first.
        if let Some(parent) = fname.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(out_dir.join(parent))?;
        }
        let path = out_dir.join(fname);
        fs::write(&path, code)?;
        paths.insert(path);
    }

    // TODO: We should probably just delete all generated files before we generate any, rather than selectively deleting some afterward.
    let mut auto_generated_buf: [u8; AUTO_GENERATED_PREFIX.len()] = [0; AUTO_GENERATED_PREFIX.len()];
    let files_to_delete = walkdir::WalkDir::new(out_dir)
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

    if let Err(err) = lang.format_files(paths) {
        // If we couldn't format the files, print a warning but don't fail the entire
        // task as the output should still be usable, just less pretty.
        eprintln!("Could not format generated files: {err}");
    }

    println!("Generate finished successfully.");
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
pub enum Language {
    Csharp,
    Go,
    TypeScript,
    Rust,
}

impl clap::ValueEnum for Language {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Csharp, Self::Go, Self::TypeScript, Self::Rust]
    }
    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(match self {
            Self::Csharp => clap::builder::PossibleValue::new("csharp").aliases(["c#", "cs"]),
            Self::Go => clap::builder::PossibleValue::new("go").aliases(["golang"]),
            Self::TypeScript => clap::builder::PossibleValue::new("typescript").aliases(["ts", "TS"]),
            Self::Rust => clap::builder::PossibleValue::new("rust").aliases(["rs", "RS"]),
        })
    }
}

impl Language {
    fn format_files(&self, generated_files: BTreeSet<PathBuf>) -> anyhow::Result<()> {
        match self {
            Language::Rust => rustfmt(generated_files)?,
            Language::Csharp => dotnet_format(generated_files)?,
            Language::Go => go_format(generated_files)?,
            Language::TypeScript => {
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
