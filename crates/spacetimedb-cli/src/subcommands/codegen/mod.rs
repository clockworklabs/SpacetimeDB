use std::fs;
use std::path::Path;

use crate::util;
use clap::Arg;
use convert_case::{Case, Casing};
use spacetimedb_lib::type_def::{ReducerDef, TableDef};
use spacetimedb_lib::TupleDef;
use wasmtime::{ExternType, Trap, TypedFunc};

mod code_indenter;
pub mod csharp;

const INDENT: &str = "\t";

pub fn cli() -> clap::Command<'static> {
    clap::Command::new("generate")
        .about("Generate client files for a spacetime module.")
        .arg(
            Arg::new("wasm_file")
                .takes_value(true)
                .required(false)
                .long("wasm-file")
                .short('w')
                .conflicts_with("projec_path"),
        )
        .arg(
            Arg::new("project_path")
                .takes_value(true)
                .required(false)
                .long("project-path")
                .short('p')
                .default_value(".")
                .conflicts_with("wasm_file"),
        )
        .arg(
            Arg::new("out_dir")
                .takes_value(true)
                .required(true)
                .long("out-dir")
                .short('o'),
        )
        .arg(
            Arg::new("lang")
                .takes_value(true)
                .required(true)
                .long("lang")
                .short('l')
                .possible_values(["csharp", "cs", "c#"]),
        )
        .after_help("Run `spacetime help publish` for more detailed information.")
}

pub fn exec(args: &clap::ArgMatches) -> anyhow::Result<()> {
    let project_path = args.value_of("project_path").unwrap();
    let wasm_file_path = Path::new(project_path);
    let wasm_file_path = util::find_wasm_file(wasm_file_path)?;
    let wasm_file = match args.value_of("wasm_file") {
        None => Path::new(wasm_file_path.to_str().unwrap()),
        Some(path) => Path::new(path),
    };

    let out_dir = Path::new(args.value_of("out_dir").unwrap());
    let lang = args.value_of("lang").unwrap();

    if !out_dir.exists() {
        return Err(anyhow::anyhow!(
            "Output directory '{}' does not exist. Please create the directory and rerun this command.",
            out_dir.to_str().unwrap()
        ));
    }

    for (fname, code) in generate(wasm_file, lang)? {
        fs::write(out_dir.join(fname), code)?;
    }
    Ok(())
}

pub fn generate(wasm_file: &Path, lang: &str) -> anyhow::Result<impl Iterator<Item = (String, String)>> {
    match lang {
        "csharp" | "cs" | "c#" => {
            let descriptions = extract_descriptions(&wasm_file)?;
            Ok(descriptions.into_iter().map(|(name, desc)| match desc {
                Description::Table(table) => {
                    let code = csharp::autogen_csharp_table(&name, &table);
                    (name + ".cs", code)
                }
                Description::Tuple(tup) => {
                    let code = csharp::autogen_csharp_tuple(&name, &tup);
                    (name + ".cs", code)
                }
                Description::Reducer(reducer) => {
                    let code = csharp::autogen_csharp_reducer(&reducer);
                    let pascalcase = name.to_case(Case::Pascal);
                    (pascalcase + "Reducer.cs", code)
                }
            }))
        }
        &_ => Err(anyhow::anyhow!(format!("Unsupported langauge: {}", lang))),
    }
}

enum Description {
    Table(TableDef),
    Tuple(TupleDef),
    Reducer(ReducerDef),
}

fn extract_descriptions(wasm_file: &Path) -> anyhow::Result<Vec<(String, Description)>> {
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::from_file(&engine, wasm_file)?;
    let mut store = wasmtime::Store::new(&engine, ());
    let mut linker = wasmtime::Linker::new(&engine);
    for imp in module.imports() {
        if let ExternType::Func(func_type) = imp.ty() {
            linker
                .func_new(imp.module(), imp.name(), func_type, |_, _, _| {
                    Err(Trap::new("don't call me!!"))
                })
                .unwrap();
        }
    }
    let instance = linker.instantiate(&mut store, &module)?;
    let memory = instance.get_memory(&mut store, "memory").unwrap();
    // let alloc: TypedFunc<(u32,), (u32,)> = instance.get_func(&mut store, "alloc").unwrap().typed(&store).unwrap();
    let dealloc: TypedFunc<(u32, u32), ()> = instance.get_func(&mut store, "dealloc").unwrap().typed(&store).unwrap();
    enum DescrType {
        Table,
        Tuple,
        Reducer,
    }
    let describes = instance
        .exports(&mut store)
        .filter_map(|exp| {
            let sym = exp.name();
            None.or_else(|| sym.strip_prefix("__describe_table__").map(|n| (DescrType::Table, n)))
                .or_else(|| sym.strip_prefix("__describe_tuple__").map(|n| (DescrType::Tuple, n)))
                .or_else(|| {
                    sym.strip_prefix("__describe_reducer__")
                        .map(|n| (DescrType::Reducer, n))
                })
                .map(|(ty, name)| (ty, name.to_owned(), exp.into_func().unwrap()))
        })
        .collect::<Vec<_>>();
    let mut descriptions = Vec::with_capacity(describes.len());
    for (ty, name, describe) in describes {
        let describe: TypedFunc<(), (u64,)> = describe.typed(&store).unwrap();
        let (val,) = describe.call(&mut store, ()).unwrap();
        let offset = (val >> 32) as u32;
        let len = (val & 0xFFFF_FFFF) as u32;
        let slice = &memory.data(&store)[offset as usize..][..len as usize];
        let descr = match ty {
            DescrType::Table => Description::Table(TableDef::decode(&mut &slice[..])?),
            DescrType::Tuple => Description::Tuple(TupleDef::decode(&mut &slice[..])?),
            DescrType::Reducer => Description::Reducer(ReducerDef::decode(&mut &slice[..])?),
        };
        dealloc.call(&mut store, (offset, len)).unwrap();
        descriptions.push((name, descr));
    }
    Ok(descriptions)
}
