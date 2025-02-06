#![warn(clippy::uninlined_format_args)]

use clap::parser::ValueSource;
use clap::Arg;
use clap::ArgAction::Set;
use core::mem;
use fs_err as fs;
use spacetimedb::host::wasmtime::{Mem, MemView, WasmPointee as _};
use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::{bsatn, RawModuleDefV8};
use spacetimedb_lib::{RawModuleDef, MODULE_ABI_MAJOR_VERSION};
use spacetimedb_primitives::errno;
use spacetimedb_schema;
use spacetimedb_schema::def::{ModuleDef, ReducerDef, ScopedTypeName, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use std::path::{Path, PathBuf};
use wasmtime::{Caller, StoreContextMut};

use crate::generate::util::iter_reducers;
use crate::util::y_or_n;
use crate::Config;
use crate::{build, common_args};
use clap::builder::PossibleValue;
use std::collections::BTreeSet;
use std::io::Read;
use util::AUTO_GENERATED_PREFIX;

mod code_indenter;
pub mod csharp;
pub mod rust;
pub mod typescript;
mod util;

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
                .help("Options to pass to the build command, for example --build-options='--skip-println-checks'"),
        )
        .arg(common_args::yes())
        .after_help("Run `spacetime help publish` for more detailed information.")
}

pub async fn exec(config: Config, args: &clap::ArgMatches) -> anyhow::Result<()> {
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

    let module = if let Some(mut json_module) = json_module {
        let DeserializeWrapper(module) = if let Some(path) = json_module.next() {
            serde_json::from_slice(&fs::read(path)?)?
        } else {
            serde_json::from_reader(std::io::stdin().lock())?
        };
        module
    } else {
        let wasm_path = if let Some(path) = wasm_file {
            println!("Skipping build. Instead we are inspecting {}", path.display());
            path.clone()
        } else {
            build::exec_with_argstring(config.clone(), project_path, build_options).await?
        };
        let spinner = indicatif::ProgressBar::new_spinner();
        spinner.enable_steady_tick(60);
        spinner.set_message("Compiling wasm...");
        let module = compile_wasm(&wasm_path)?;
        spinner.set_message("Extracting schema from wasm...");
        extract_descriptions_from_module(module)?
    };

    fs::create_dir_all(out_dir)?;

    let mut paths = BTreeSet::new();

    let csharp_lang;
    let lang = match lang {
        Language::Csharp => {
            csharp_lang = csharp::Csharp { namespace };
            &csharp_lang as &dyn Lang
        }
        Language::Rust => &rust::Rust,
        Language::TypeScript => &typescript::TypeScript,
    };

    for (fname, code) in generate(module, lang)? {
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
    TypeScript,
    Rust,
}

impl clap::ValueEnum for Language {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Csharp, Self::TypeScript, Self::Rust]
    }
    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(match self {
            Self::Csharp => csharp::Csharp::clap_value(),
            Self::TypeScript => typescript::TypeScript::clap_value(),
            Self::Rust => rust::Rust::clap_value(),
        })
    }
}

pub fn generate(module: RawModuleDef, lang: &dyn Lang) -> anyhow::Result<Vec<(String, String)>> {
    let module = &ModuleDef::try_from(module)?;
    Ok(itertools::chain!(
        module
            .tables()
            .map(|tbl| { (lang.table_filename(module, tbl), lang.generate_table(module, tbl),) }),
        module
            .types()
            .map(|typ| { (lang.type_filename(&typ.name), lang.generate_type(module, typ),) }),
        iter_reducers(module).map(|reducer| {
            (
                lang.reducer_filename(&reducer.name),
                lang.generate_reducer(module, reducer),
            )
        }),
        lang.generate_globals(module),
    )
    .collect())
}

pub trait Lang {
    fn table_filename(&self, module: &ModuleDef, table: &TableDef) -> String;
    fn type_filename(&self, type_name: &ScopedTypeName) -> String;
    fn reducer_filename(&self, reducer_name: &Identifier) -> String;

    fn generate_table(&self, module: &ModuleDef, tbl: &TableDef) -> String;
    fn generate_type(&self, module: &ModuleDef, typ: &TypeDef) -> String;
    fn generate_reducer(&self, module: &ModuleDef, reducer: &ReducerDef) -> String;
    fn generate_globals(&self, module: &ModuleDef) -> Vec<(String, String)>;

    fn format_files(&self, generated_files: BTreeSet<PathBuf>) -> anyhow::Result<()>;
    fn clap_value() -> PossibleValue
    where
        Self: Sized;
}

pub fn extract_descriptions(wasm_file: &Path) -> anyhow::Result<RawModuleDef> {
    let module = compile_wasm(wasm_file)?;
    extract_descriptions_from_module(module)
}

fn compile_wasm(wasm_file: &Path) -> anyhow::Result<wasmtime::Module> {
    wasmtime::Module::from_file(&wasmtime::Engine::default(), wasm_file)
}

fn extract_descriptions_from_module(module: wasmtime::Module) -> anyhow::Result<RawModuleDef> {
    let engine = module.engine();
    let ctx = WasmCtx {
        mem: None,
        sink: Vec::new(),
    };
    let mut store = wasmtime::Store::new(engine, ctx);
    let mut linker = wasmtime::Linker::new(engine);
    linker.allow_shadowing(true).define_unknown_imports_as_traps(&module)?;
    let module_name = &*format!("spacetime_{MODULE_ABI_MAJOR_VERSION}.0");
    linker.func_wrap(
        module_name,
        "console_log",
        |mut caller: Caller<'_, WasmCtx>,
         _level: u32,
         _target_ptr: u32,
         _target_len: u32,
         _filename_ptr: u32,
         _filename_len: u32,
         _line_number: u32,
         message_ptr: u32,
         message_len: u32| {
            let (mem, _) = WasmCtx::mem_env(&mut caller);
            let slice = mem.deref_slice(message_ptr, message_len).unwrap();
            println!("from wasm: {}", String::from_utf8_lossy(slice));
        },
    )?;
    linker.func_wrap(module_name, "bytes_sink_write", WasmCtx::bytes_sink_write)?;
    let instance = linker.instantiate(&mut store, &module)?;
    let memory = Mem::extract(&instance, &mut store)?;
    store.data_mut().mem = Some(memory);

    let mut preinits = instance
        .exports(&mut store)
        .filter_map(|exp| Some((exp.name().strip_prefix("__preinit__")?.to_owned(), exp.into_func()?)))
        .collect::<Vec<_>>();
    preinits.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (_, func) in preinits {
        func.typed(&store)?.call(&mut store, ())?
    }
    let module: RawModuleDef = match instance.get_func(&mut store, "__describe_module__") {
        Some(f) => {
            store.data_mut().sink = Vec::new();
            f.typed::<u32, ()>(&store)?.call(&mut store, 1)?;
            let buf = mem::take(&mut store.data_mut().sink);
            bsatn::from_slice(&buf)?
        }
        // TODO: shouldn't we return an error here?
        None => RawModuleDef::V8BackCompat(RawModuleDefV8::default()),
    };
    Ok(module)
}

struct WasmCtx {
    mem: Option<Mem>,
    sink: Vec<u8>,
}

impl WasmCtx {
    pub fn get_mem(&self) -> Mem {
        self.mem.expect("Initialized memory")
    }

    fn mem_env<'a>(ctx: impl Into<StoreContextMut<'a, Self>>) -> (&'a mut MemView, &'a mut Self) {
        let ctx = ctx.into();
        let mem = ctx.data().get_mem();
        mem.view_and_store_mut(ctx)
    }

    pub fn bytes_sink_write(
        mut caller: Caller<'_, Self>,
        sink_handle: u32,
        buffer_ptr: u32,
        buffer_len_ptr: u32,
    ) -> anyhow::Result<u32> {
        if sink_handle != 1 {
            return Ok(errno::NO_SUCH_BYTES.get().into());
        }

        let (mem, env) = Self::mem_env(&mut caller);

        // Read `buffer_len`, i.e., the capacity of `buffer` pointed to by `buffer_ptr`.
        let buffer_len = u32::read_from(mem, buffer_len_ptr)?;
        // Write `buffer` to `sink`.
        let buffer = mem.deref_slice(buffer_ptr, buffer_len)?;
        env.sink.extend(buffer);

        Ok(0)
    }
}
