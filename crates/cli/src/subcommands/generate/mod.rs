#![warn(clippy::uninlined_format_args)]

use clap::parser::ValueSource;
use clap::Arg;
use clap::ArgAction::Set;
use convert_case::{Case, Casing};
use core::mem;
use duct::cmd;
use itertools::Itertools;
use spacetimedb::host::wasmtime::{Mem, MemView, WasmPointee as _};
use spacetimedb_data_structures::map::HashSet;
use spacetimedb_lib::de::serde::DeserializeWrapper;
use spacetimedb_lib::sats::{AlgebraicType, AlgebraicTypeRef, Typespace};
use spacetimedb_lib::{bsatn, RawModuleDefV8, TypeAlias};
use spacetimedb_lib::{RawModuleDef, MODULE_ABI_MAJOR_VERSION};
use spacetimedb_primitives::errno;
use spacetimedb_schema;
use spacetimedb_schema::def::{ModuleDef, ReducerDef, ScopedTypeName, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use spacetimedb_schema::schema::{Schema, TableSchema};
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use wasmtime::{Caller, StoreContextMut};

use crate::detect::{has_rust_fmt, has_rust_up};
use crate::util::y_or_n;
use crate::Config;
use crate::{build, common_args};

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
            serde_json::from_slice(&std::fs::read(path)?)?
        } else {
            serde_json::from_reader(std::io::stdin().lock())?
        };
        module
    } else {
        let wasm_path = if !project_path.is_dir() && project_path.extension().map_or(false, |ext| ext == "wasm") {
            println!("Note: Using --project-path to provide a wasm file is deprecated, and will be");
            println!("removed in a future release. Please use --bin-path instead.");
            project_path.clone()
        } else if let Some(path) = wasm_file {
            println!("Skipping build. Instead we are inspecting {}", path.display());
            path.clone()
        } else {
            build::exec_with_argstring(config.clone(), project_path, build_options).await?
        };
        extract_descriptions(&wasm_path)?
    };

    fs::create_dir_all(out_dir)?;

    let mut paths = vec![];
    for (fname, code) in generate(module, lang, namespace.as_str())? {
        let fname = Path::new(&fname);
        // If a generator asks for a file in a subdirectory, create the subdirectory first.
        if let Some(parent) = fname.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(out_dir.join(parent))?;
        }
        let path = out_dir.join(fname);
        paths.push(path.clone());
        fs::write(path, code)?;
    }

    format_files(paths.clone(), lang)?;

    // TODO: We should probably just delete all generated files before we generate any, rather than selectively deleting some afterward.
    let mut files_to_delete = vec![];
    for entry in fs::read_dir(out_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Ok(contents) = fs::read_to_string(&path) {
                if !contents.starts_with("// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB.") {
                    continue;
                }
            }

            if paths
                .iter()
                .any(|x| x.file_name().unwrap() == path.file_name().unwrap())
            {
                continue;
            }
            files_to_delete.push(path);
        }
    }
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
    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            Self::Csharp => Some(clap::builder::PossibleValue::new("csharp").aliases(["c#", "cs"])),
            Self::TypeScript => Some(clap::builder::PossibleValue::new("typescript").aliases(["ts", "TS"])),
            Self::Rust => Some(clap::builder::PossibleValue::new("rust").aliases(["rs", "RS"])),
        }
    }
}

pub struct GenCtx {
    typespace: Typespace,
    names: Vec<Option<String>>,
}

pub fn generate(module: RawModuleDef, lang: Language, namespace: &str) -> anyhow::Result<Vec<(String, String)>> {
    let module = ModuleDef::try_from(module)?;
    Ok(match lang {
        Language::Rust => generate_lang(&module, rust::Rust, namespace),
        Language::TypeScript => generate_lang(&module, typescript::TypeScript, namespace),
        Language::Csharp => {
            let ctx = GenCtx {
                typespace: module.typespace().clone(),
                names: (0..module.typespace().types.len())
                    .map(|r| {
                        module
                            .type_def_from_ref(AlgebraicTypeRef(r as _))
                            .map(|(name, _)| name.name_segments().join("."))
                    })
                    .collect(),
            };

            let tableset = module.tables().map(|t| t.product_type_ref).collect::<HashSet<_>>();
            let tables = module
                .tables()
                .map(|table| TableDescHack {
                    schema: TableSchema::from_module_def(&module, table, (), 0.into()),
                    data: table.product_type_ref,
                })
                .sorted_by(|a, b| a.schema.table_name.cmp(&b.schema.table_name));

            // HACK: Patch the fields to have the types that point to `AlgebraicTypeRef` because all generators depend on that
            // `register_table` in rt.rs resolve the types early, but the generators do it late. This impact enums where
            // the enum name is not preserved in the `AlgebraicType`.
            // x.schema.columns =
            //     RawColumnDefV8::from_product_type(typespace[x.data].as_product().unwrap().clone());

            let types = module.types().filter(|typ| !tableset.contains(&typ.ty)).map(|typ| {
                GenItem::TypeAlias(TypeAlias {
                    name: typ.name.name_segments().join("."),
                    ty: typ.ty,
                })
            });

            let reducers = module
                .reducers()
                .map(|reducer| spacetimedb_lib::ReducerDef {
                    name: reducer.name.clone().into(),
                    args: reducer.params.elements.to_vec(),
                })
                .sorted_by(|a, b| a.name.cmp(&b.name));

            let items = itertools::chain!(
                types,
                tables.into_iter().map(GenItem::Table),
                reducers
                    .filter(|r| !(r.name.starts_with("__") && r.name.ends_with("__")))
                    .map(GenItem::Reducer),
            );

            let items: Vec<GenItem> = items.collect();
            let mut files: Vec<(String, String)> = items
                .iter()
                .filter_map(|item| item.generate(&ctx, lang, namespace))
                .collect();
            files.extend(generate_globals(&ctx, lang, namespace, &items));
            files
        }
    })
}

fn generate_lang(module: &ModuleDef, lang: impl Lang, namespace: &str) -> Vec<(String, String)> {
    itertools::chain!(
        module.tables().map(|tbl| {
            (
                lang.table_filename(module, tbl),
                lang.generate_table(module, namespace, tbl),
            )
        }),
        module.types().map(|typ| {
            (
                lang.type_filename(&typ.name),
                lang.generate_type(module, namespace, typ),
            )
        }),
        module.reducers().map(|reducer| {
            (
                lang.reducer_filename(&reducer.name),
                lang.generate_reducer(module, namespace, reducer),
            )
        }),
        lang.generate_globals(module, namespace),
    )
    .collect()
}

trait Lang {
    fn table_filename(&self, module: &ModuleDef, table: &TableDef) -> String;
    fn type_filename(&self, type_name: &ScopedTypeName) -> String;
    fn reducer_filename(&self, reducer_name: &Identifier) -> String;

    fn generate_table(&self, module: &ModuleDef, namespace: &str, tbl: &TableDef) -> String;
    fn generate_type(&self, module: &ModuleDef, namespace: &str, typ: &TypeDef) -> String;
    fn generate_reducer(&self, module: &ModuleDef, namespace: &str, reducer: &ReducerDef) -> String;
    fn generate_globals(&self, module: &ModuleDef, namespace: &str) -> Vec<(String, String)>;
}

/// Backwards-compatibible imitation of `TableDesc` that should be removed once the generators are updated to rely on `ModuleDef`.
pub struct TableDescHack {
    schema: TableSchema,
    data: AlgebraicTypeRef,
}

pub enum GenItem {
    Table(TableDescHack),
    TypeAlias(TypeAlias),
    Reducer(spacetimedb_lib::ReducerDef),
}

fn generate_globals(ctx: &GenCtx, lang: Language, namespace: &str, items: &[GenItem]) -> Vec<(String, String)> {
    match lang {
        Language::Csharp => csharp::autogen_csharp_globals(ctx, items, namespace),
        Language::TypeScript => unreachable!(),
        Language::Rust => unreachable!(),
    }
}

impl GenItem {
    fn generate(&self, ctx: &GenCtx, lang: Language, namespace: &str) -> Option<(String, String)> {
        match lang {
            Language::Csharp => self.generate_csharp(ctx, namespace),
            Language::TypeScript => unreachable!(),
            Language::Rust => unreachable!(),
        }
    }

    fn generate_csharp(&self, ctx: &GenCtx, namespace: &str) -> Option<(String, String)> {
        match self {
            GenItem::Table(table) => {
                let code = csharp::autogen_csharp_table(ctx, table, namespace);
                Some((table.schema.table_name.as_ref().to_case(Case::Pascal) + ".cs", code))
            }
            GenItem::TypeAlias(TypeAlias { name, ty }) => match &ctx.typespace[*ty] {
                AlgebraicType::Sum(sum) => {
                    let filename = name.replace('.', "");
                    let code = csharp::autogen_csharp_sum(ctx, name, sum, namespace);
                    Some((filename + ".cs", code))
                }
                AlgebraicType::Product(prod) => {
                    let code = csharp::autogen_csharp_tuple(ctx, name, prod, namespace);
                    Some((name.clone() + ".cs", code))
                }
                _ => todo!(),
            },
            GenItem::Reducer(reducer) => {
                let code = csharp::autogen_csharp_reducer(ctx, reducer, namespace);
                let pascalcase = reducer.name.deref().to_case(Case::Pascal);
                Some((pascalcase + "Reducer.cs", code))
            }
        }
    }
}

pub fn extract_descriptions(wasm_file: &Path) -> anyhow::Result<RawModuleDef> {
    let engine = wasmtime::Engine::default();
    let t = std::time::Instant::now();
    let module = wasmtime::Module::from_file(&engine, wasm_file)?;
    println!("compilation took {:?}", t.elapsed());
    let ctx = WasmCtx {
        mem: None,
        sink: Vec::new(),
    };
    let mut store = wasmtime::Store::new(&engine, ctx);
    let mut linker = wasmtime::Linker::new(&engine);
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
            f.typed::<u32, ()>(&store)?.call(&mut store, 1).unwrap();
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

fn format_files(generated_files: Vec<PathBuf>, lang: Language) -> anyhow::Result<()> {
    match lang {
        Language::Rust => {
            if !has_rust_fmt() {
                if has_rust_up() {
                    if let Err(err) = cmd!("rustup", "component", "add", "rustfmt").run() {
                        println!("Warning: Failed to install rustfmt: {err}");
                    }
                } else {
                    return Err(anyhow::anyhow!("rustfmt is not installed. Please install it."));
                }
            }
            for path in generated_files {
                cmd!("rustfmt", "--edition", "2021", path.to_str().unwrap()).run()?;
            }
        }
        Language::Csharp => {}
        Language::TypeScript => {}
    }

    Ok(())
}
