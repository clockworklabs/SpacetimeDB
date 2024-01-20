use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::Arg;
use clap::ArgAction::SetTrue;
use convert_case::{Case, Casing};
use duct::cmd;
use spacetimedb_lib::sats::{AlgebraicType, Typespace};
use spacetimedb_lib::MODULE_ABI_MAJOR_VERSION;
use spacetimedb_lib::{bsatn, MiscModuleExport, ModuleDef, ReducerDef, TableDesc, TypeAlias};
use wasmtime::{AsContext, Caller};

mod code_indenter;
pub mod csharp;
pub mod python;
pub mod rust;
pub mod typescript;
mod util;

const INDENT: &str = "\t";

pub fn cli() -> clap::Command {
    clap::Command::new("generate")
        .about("Generate client files for a spacetime module.")
        .arg(
            Arg::new("wasm_file")
                .value_parser(clap::value_parser!(PathBuf))
                .long("wasm-file")
                .short('w')
                .conflicts_with("project_path")
                .help("The system path (absolute or relative) to the wasm file we should inspect"),
        )
        .arg(
            Arg::new("project_path")
                .value_parser(clap::value_parser!(PathBuf))
                .long("project-path")
                .short('p')
                .default_value(".")
                .conflicts_with("wasm_file")
                .help("The path to the wasm project"),
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
                .short('n')
                .help("The namespace that should be used (default is 'SpacetimeDB.Types')"),
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
            Arg::new("skip_clippy")
                .long("skip_clippy")
                .short('s')
                .short('S')
                .action(SetTrue)
                .env("SPACETIME_SKIP_CLIPPY")
                .value_parser(clap::builder::FalseyValueParser::new())
                .help("Skips running clippy on the module before generating (intended to speed up local iteration, not recommended for CI)"),
        )
        .arg(
            Arg::new("delete_files")
                .long("delete-files")
                .action(SetTrue)
                .help("Delete outdated generated files whose definitions have been removed from the module. Prompts before deleting unless --force is supplied."),
        )
        .arg(
            Arg::new("force")
                .long("force")
                .action(SetTrue)
                .requires("delete_files")
                .help("delete-files without prompting first. Useful for scripts."),
        )
        .arg(
            Arg::new("debug")
                .long("debug")
                .short('d')
                .action(SetTrue)
                .help("Builds the module using debug instead of release (intended to speed up local iteration, not recommended for CI)"),
        )
        .after_help("Run `spacetime help publish` for more detailed information.")
}

pub fn exec(args: &clap::ArgMatches) -> anyhow::Result<()> {
    let project_path = args.get_one::<PathBuf>("project_path").unwrap();
    let wasm_file = args.get_one::<PathBuf>("wasm_file").cloned();
    let out_dir = args.get_one::<PathBuf>("out_dir").unwrap();
    let lang = *args.get_one::<Language>("lang").unwrap();
    let namespace = args.get_one::<String>("namespace").unwrap();
    let skip_clippy = args.get_flag("skip_clippy");
    let build_debug = args.get_flag("debug");
    let delete_files = args.get_flag("delete_files");
    let force = args.get_flag("force");

    let wasm_file = match wasm_file {
        Some(x) => x,
        None => match crate::tasks::build(project_path, skip_clippy, build_debug) {
            Ok(wasm_file) => wasm_file,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "{:?}

Failed to compile module {:?}. See cargo errors above for more details.",
                    e,
                    project_path,
                ));
            }
        },
    };

    fs::create_dir_all(out_dir)?;

    let mut paths = vec![];
    for (fname, code) in generate(&wasm_file, lang, namespace.as_str())?.into_iter() {
        let path = out_dir.join(fname);
        paths.push(path.clone());
        fs::write(path, code)?;
    }

    format_files(paths.clone(), lang)?;

    if delete_files {
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
            let mut input = "y".to_string();
            println!("The following files were not generated by this command and will be deleted:");
            for path in &files_to_delete {
                println!("  {}", path.to_str().unwrap());
            }

            if !force {
                print!("Are you sure you want to delete these files? [y/N] ");
                input = "".to_string();
                std::io::stdout().flush()?;
                std::io::stdin().read_line(&mut input)?;
            } else {
                println!("Force flag present, deleting files without prompting.");
            }

            if input.trim().to_lowercase() == "y" || input.trim().to_lowercase() == "yes" {
                for path in files_to_delete {
                    fs::remove_file(path)?;
                }
                println!("Files deleted successfully.");
            } else {
                println!("Files not deleted.");
            }
        }
    }

    println!("Generate finished successfully.");
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
pub enum Language {
    Csharp,
    TypeScript,
    Python,
    Rust,
}

impl clap::ValueEnum for Language {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Csharp, Self::TypeScript, Self::Python, Self::Rust]
    }
    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            Self::Csharp => Some(clap::builder::PossibleValue::new("csharp").aliases(["c#", "cs"])),
            Self::TypeScript => Some(clap::builder::PossibleValue::new("typescript").aliases(["ts", "TS"])),
            Self::Python => Some(clap::builder::PossibleValue::new("python").aliases(["py", "PY"])),
            Self::Rust => Some(clap::builder::PossibleValue::new("rust").aliases(["rs", "RS"])),
        }
    }
}

pub struct GenCtx {
    typespace: Typespace,
    names: Vec<Option<String>>,
}

pub fn generate<'a>(wasm_file: &'a Path, lang: Language, namespace: &'a str) -> anyhow::Result<Vec<(String, String)>> {
    let module = extract_descriptions(wasm_file)?;
    let (ctx, items) = extract_from_moduledef(module);
    let items: Vec<GenItem> = items.collect();
    let mut files: Vec<(String, String)> = items
        .iter()
        .filter_map(|item| item.generate(&ctx, lang, namespace))
        .collect();
    files.extend(generate_globals(&ctx, lang, namespace, &items).into_iter().flatten());

    Ok(files)
}

fn generate_globals(ctx: &GenCtx, lang: Language, namespace: &str, items: &[GenItem]) -> Vec<Vec<(String, String)>> {
    match lang {
        Language::Csharp => csharp::autogen_csharp_globals(items, namespace),
        Language::TypeScript => typescript::autogen_typescript_globals(ctx, items),
        Language::Python => python::autogen_python_globals(ctx, items),
        Language::Rust => rust::autogen_rust_globals(ctx, items),
    }
}

pub fn extract_from_moduledef(module: ModuleDef) -> (GenCtx, impl Iterator<Item = GenItem>) {
    let ModuleDef {
        typespace,
        tables,
        reducers,
        misc_exports,
    } = module;
    // HACK: Patch the fields to have the types that point to `AlgebraicTypeRef` because all generators depend on that
    // `register_table` in rt.rs resolve the types early, but the generators do it late. This impact enums where
    // the enum name is not preserved in the `AlgebraicType`.
    let tables: Vec<_> = tables
        .into_iter()
        .map(|mut x| {
            x.schema.columns = typespace[x.data].as_product().unwrap().clone().into();
            x
        })
        .collect();

    let mut names = vec![None; typespace.types.len()];
    let name_info = itertools::chain!(
        tables.iter().map(|t| (t.data, &t.schema.table_name)),
        misc_exports
            .iter()
            .map(|MiscModuleExport::TypeAlias(a)| (a.ty, &a.name)),
    );
    for (typeref, name) in name_info {
        names[typeref.idx()] = Some(name.clone())
    }
    let ctx = GenCtx { typespace, names };
    let iter = itertools::chain!(
        misc_exports.into_iter().map(GenItem::from_misc_export),
        tables.into_iter().map(GenItem::Table),
        reducers
            .into_iter()
            .filter(|r| !(r.name.starts_with("__") && r.name.ends_with("__")))
            .map(GenItem::Reducer),
    );
    (ctx, iter)
}

pub enum GenItem {
    Table(TableDesc),
    TypeAlias(TypeAlias),
    Reducer(ReducerDef),
}

impl GenItem {
    fn from_misc_export(exp: MiscModuleExport) -> Self {
        match exp {
            MiscModuleExport::TypeAlias(a) => Self::TypeAlias(a),
        }
    }

    fn generate(&self, ctx: &GenCtx, lang: Language, namespace: &str) -> Option<(String, String)> {
        match lang {
            Language::Csharp => self.generate_csharp(ctx, namespace),
            Language::TypeScript => self.generate_typescript(ctx),
            Language::Python => self.generate_python(ctx),
            Language::Rust => self.generate_rust(ctx),
        }
    }

    fn generate_rust(&self, ctx: &GenCtx) -> Option<(String, String)> {
        match self {
            GenItem::Table(table) => {
                let code = rust::autogen_rust_table(ctx, table);
                Some((rust::rust_type_file_name(&table.schema.table_name), code))
            }
            GenItem::TypeAlias(TypeAlias { name, ty }) => {
                let code = match &ctx.typespace[*ty] {
                    AlgebraicType::Sum(sum) => rust::autogen_rust_sum(ctx, name, sum),
                    AlgebraicType::Product(prod) => rust::autogen_rust_tuple(ctx, name, prod),
                    _ => todo!(),
                };
                Some((rust::rust_type_file_name(name), code))
            }
            GenItem::Reducer(reducer) => {
                let code = rust::autogen_rust_reducer(ctx, reducer);
                Some((rust::rust_reducer_file_name(&reducer.name), code))
            }
        }
    }

    fn generate_python(&self, ctx: &GenCtx) -> Option<(String, String)> {
        match self {
            GenItem::Table(table) => {
                let code = python::autogen_python_table(ctx, table);
                let name = table.schema.table_name.to_case(Case::Snake);
                Some((name + ".py", code))
            }
            GenItem::TypeAlias(TypeAlias { name, ty }) => match &ctx.typespace[*ty] {
                AlgebraicType::Sum(sum) => {
                    let filename = name.replace('.', "").to_case(Case::Snake);
                    let code = python::autogen_python_sum(ctx, name, sum);
                    Some((filename + ".py", code))
                }
                AlgebraicType::Product(prod) => {
                    let code = python::autogen_python_tuple(ctx, name, prod);
                    let name = name.to_case(Case::Snake);
                    Some((name + ".py", code))
                }
                AlgebraicType::Builtin(_) => todo!(),
                AlgebraicType::Ref(_) => todo!(),
            },
            GenItem::Reducer(reducer) => {
                let code = python::autogen_python_reducer(ctx, reducer);
                let name = reducer.name.to_case(Case::Snake);
                Some((name + "_reducer.py", code))
            }
        }
    }

    fn generate_typescript(&self, ctx: &GenCtx) -> Option<(String, String)> {
        match self {
            GenItem::Table(table) => {
                let code = typescript::autogen_typescript_table(ctx, table);
                let name = table.schema.table_name.to_case(Case::Snake);
                Some((name + ".ts", code))
            }
            GenItem::TypeAlias(TypeAlias { name, ty }) => match &ctx.typespace[*ty] {
                AlgebraicType::Sum(sum) => {
                    let filename = name.replace('.', "").to_case(Case::Snake);
                    let code = typescript::autogen_typescript_sum(ctx, name, sum);
                    Some((filename + ".ts", code))
                }
                AlgebraicType::Product(prod) => {
                    let code = typescript::autogen_typescript_tuple(ctx, name, prod);
                    let name = name.to_case(Case::Snake);
                    Some((name + ".ts", code))
                }
                AlgebraicType::Builtin(_) => todo!(),
                AlgebraicType::Ref(_) => todo!(),
            },
            GenItem::Reducer(reducer) => {
                let code = typescript::autogen_typescript_reducer(ctx, reducer);
                let name = reducer.name.to_case(Case::Snake);
                Some((name + "_reducer.ts", code))
            }
        }
    }

    fn generate_csharp(&self, ctx: &GenCtx, namespace: &str) -> Option<(String, String)> {
        match self {
            GenItem::Table(table) => {
                let code = csharp::autogen_csharp_table(ctx, table, namespace);
                Some((table.schema.table_name.clone() + ".cs", code))
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
                AlgebraicType::Builtin(_) => todo!(),
                AlgebraicType::Ref(_) => todo!(),
            },
            GenItem::Reducer(reducer) => {
                let code = csharp::autogen_csharp_reducer(ctx, reducer, namespace);
                let pascalcase = reducer.name.to_case(Case::Pascal);
                Some((pascalcase + "Reducer.cs", code))
            }
        }
    }
}

fn extract_descriptions(wasm_file: &Path) -> anyhow::Result<ModuleDef> {
    let engine = wasmtime::Engine::default();
    let t = std::time::Instant::now();
    let module = wasmtime::Module::from_file(&engine, wasm_file)?;
    println!("compilation took {:?}", t.elapsed());
    let ctx = WasmCtx {
        mem: None,
        buffers: slab::Slab::new(),
    };
    let mut store = wasmtime::Store::new(&engine, ctx);
    let mut linker = wasmtime::Linker::new(&engine);
    linker.allow_shadowing(true).define_unknown_imports_as_traps(&module)?;
    let module_name = &*format!("spacetime_{MODULE_ABI_MAJOR_VERSION}.0");
    linker.func_wrap(
        module_name,
        "_console_log",
        |caller: Caller<'_, WasmCtx>,
         _level: u32,
         _target: u32,
         _target_len: u32,
         _filename: u32,
         _filename_len: u32,
         _line_number: u32,
         message: u32,
         message_len: u32| {
            let mem = caller.data().mem.unwrap();
            let slice = mem.deref_slice(&caller, message, message_len);
            if let Some(slice) = slice {
                println!("from wasm: {}", String::from_utf8_lossy(slice));
            } else {
                println!("tried to print from wasm but out of bounds")
            }
        },
    )?;
    linker.func_wrap(module_name, "_buffer_alloc", WasmCtx::buffer_alloc)?;
    let instance = linker.instantiate(&mut store, &module)?;
    let memory = Memory {
        mem: instance.get_memory(&mut store, "memory").unwrap(),
    };
    store.data_mut().mem = Some(memory);

    let mut preinits = instance
        .exports(&mut store)
        .filter_map(|exp| Some((exp.name().strip_prefix("__preinit__")?.to_owned(), exp.into_func()?)))
        .collect::<Vec<_>>();
    preinits.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (_, func) in preinits {
        func.typed(&store)?.call(&mut store, ())?
    }
    let module = match instance.get_func(&mut store, "__describe_module__") {
        Some(f) => {
            let buf: u32 = f.typed(&store)?.call(&mut store, ()).unwrap();
            let slice = store.data_mut().buffers.remove(buf as usize);
            bsatn::from_slice(&slice)?
        }
        None => ModuleDef::default(),
    };
    Ok(module)
}

struct WasmCtx {
    mem: Option<Memory>,
    buffers: slab::Slab<Vec<u8>>,
}

impl WasmCtx {
    fn mem(&self) -> Memory {
        self.mem.unwrap()
    }
    fn buffer_alloc(mut caller: Caller<'_, Self>, data: u32, data_len: u32) -> u32 {
        let buf = caller
            .data()
            .mem()
            .deref_slice(&caller, data, data_len)
            .unwrap()
            .to_vec();
        caller.data_mut().buffers.insert(buf) as u32
    }
}

#[derive(Copy, Clone)]
struct Memory {
    mem: wasmtime::Memory,
}

impl Memory {
    fn deref_slice<'a>(&self, store: &'a impl AsContext, offset: u32, len: u32) -> Option<&'a [u8]> {
        self.mem
            .data(store.as_context())
            .get(offset as usize..)?
            .get(..len as usize)
    }
}

fn format_files(generated_files: Vec<PathBuf>, lang: Language) -> anyhow::Result<()> {
    match lang {
        Language::Rust => {
            cmd!("rustup", "component", "add", "rustfmt").run()?;
            for path in generated_files {
                cmd!("rustfmt", path.to_str().unwrap()).run()?;
            }
        }
        Language::Csharp => {}
        Language::TypeScript => {}
        Language::Python => {}
    }

    Ok(())
}
