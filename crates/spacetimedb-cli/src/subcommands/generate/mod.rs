use std::fs;
use std::path::Path;

use crate::util;
use anyhow::Context as _;
use clap::Arg;
use convert_case::{Case, Casing};
use duckscript::types::runtime::{Context, StateValue};
use spacetimedb_lib::{EntityDef, ModuleItemDef, ReducerDef, RepeaterDef, TableDef, TupleDef};
use wasmtime::{AsContext, AsContextMut, Caller, ExternType, Trap, TypedFunc};

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
                .conflicts_with("project_path"),
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
                .value_parser(clap::value_parser!(Language)),
        )
        .after_help("Run `spacetime help publish` for more detailed information.")
}

pub fn exec(args: &clap::ArgMatches) -> anyhow::Result<()> {
    let project_path = args.value_of("project_path").unwrap();
    let wasm_file_path = Path::new(project_path);
    let mut context = Context::new();
    duckscriptsdk::load(&mut context.commands)?;
    context
        .variables
        .insert("PATH".to_string(), std::env::var("PATH").unwrap());
    context
        .variables
        .insert("PROJECT_PATH".to_string(), project_path.to_string());

    match util::invoke_duckscript(include_str!("../project/build.duck"), context) {
        Ok(ok) => {
            let mut error = false;
            for entry in ok.state {
                if let StateValue::SubState(sub_state) = entry.1 {
                    for entry in sub_state {
                        match entry.1 {
                            StateValue::String(a) => {
                                error = true;
                                println!("{}|{}", entry.0, a)
                            }
                            _ => {}
                        }
                    }
                }
            }

            if error {
                return Err(anyhow::anyhow!(
                    "Build finished with errors, check the console for more information."
                ));
            }
        }
        Err(e) => return Err(anyhow::anyhow!(format!("Script execution error: {}", e))),
    }

    let wasm_file_path = util::find_wasm_file(wasm_file_path)?;
    let wasm_file = match args.value_of("wasm_file") {
        None => Path::new(wasm_file_path.to_str().unwrap()),
        Some(path) => Path::new(path),
    };

    let out_dir = Path::new(args.value_of("out_dir").unwrap());
    let lang = *args.get_one::<Language>("lang").unwrap();
    if !out_dir.exists() {
        return Err(anyhow::anyhow!(
            "Output directory '{}' does not exist. Please create the directory and rerun this command.",
            out_dir.to_str().unwrap()
        ));
    }

    for (fname, code) in generate(wasm_file, lang)? {
        fs::write(out_dir.join(fname), code)?;
    }

    println!("Generate finished successfully.");
    Ok(())
}

#[derive(Clone, Copy)]
pub enum Language {
    Csharp,
}
impl clap::ValueEnum for Language {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Csharp]
    }
    fn to_possible_value<'a>(&self) -> Option<clap::PossibleValue<'a>> {
        match self {
            Self::Csharp => Some(clap::PossibleValue::new("csharp").aliases(["c#", "cs"])),
        }
    }
}

pub fn generate(wasm_file: &Path, lang: Language) -> anyhow::Result<impl Iterator<Item = (String, String)>> {
    let Language::Csharp = lang;
    let descriptions = extract_descriptions(&wasm_file)?;
    Ok(descriptions.into_iter().filter_map(|(name, desc)| match desc {
        ModuleItemDef::Entity(EntityDef::Table(table)) => {
            let code = csharp::autogen_csharp_table(&name, &table);
            Some((name + ".cs", code))
        }
        ModuleItemDef::Tuple(tup) => {
            let code = csharp::autogen_csharp_tuple(&name, &tup);
            Some((name + ".cs", code))
        }
        ModuleItemDef::Entity(EntityDef::Reducer(reducer)) => {
            let code = csharp::autogen_csharp_reducer(&reducer);
            let pascalcase = name.to_case(Case::Pascal);
            Some((pascalcase + "Reducer.cs", code))
        }
        ModuleItemDef::Entity(EntityDef::Repeater(_)) => {
            // Nothing to codegen for this (yet?)
            None
        }
    }))
}

fn extract_descriptions(wasm_file: &Path) -> anyhow::Result<Vec<(String, ModuleItemDef)>> {
    let engine = wasmtime::Engine::default();
    let t = std::time::Instant::now();
    let module = wasmtime::Module::from_file(&engine, wasm_file)?;
    println!("compilation took {:?}", t.elapsed());
    let mut store = wasmtime::Store::new(&engine, WasmCtx { mem: None });
    let mut linker = wasmtime::Linker::new(&engine);
    linker.allow_shadowing(true);
    for imp in module.imports() {
        if let ExternType::Func(func_type) = imp.ty() {
            linker
                .func_new(imp.module(), imp.name(), func_type, |_, _, _| {
                    Err(Trap::new("don't call me!!"))
                })
                .unwrap();
        }
    }
    linker.func_wrap(
        "env",
        "_console_log",
        |caller: Caller<'_, WasmCtx>, _level: u32, ptr: u32, len: u32| {
            let mem = caller.data().mem.unwrap();
            let slice = mem.deref_slice(&caller, ptr, len);
            if let Some(slice) = slice {
                println!("from wasm: {}", String::from_utf8_lossy(slice));
            } else {
                println!("tried to print from wasm but out of bounds")
            }
        },
    )?;
    let instance = linker.instantiate(&mut store, &module)?;
    let memory = Memory {
        mem: instance.get_memory(&mut store, "memory").unwrap(),
        dealloc: instance.get_func(&mut store, "dealloc").unwrap().typed(&store).unwrap(),
    };
    store.data_mut().mem = Some(memory);
    // let alloc: TypedFunc<(u32,), (u32,)> = instance.get_func(&mut store, "alloc").unwrap().typed(&store).unwrap();
    enum DescrType {
        Table,
        Tuple,
        Reducer,
        Repeater,
    }
    let describes = instance
        .exports(&mut store)
        .filter_map(|exp| {
            let sym = exp.name();
            let func = exp.into_func()?;
            let prefixes = [
                ("__describe_table__", DescrType::Table),
                ("__describe_tuple__", DescrType::Tuple),
                ("__describe_reducer__", DescrType::Reducer),
                ("__describe_repeating_reducer__", DescrType::Repeater),
            ];
            prefixes
                .into_iter()
                .find_map(|(prefix, ty)| sym.strip_prefix(prefix).map(|name| (ty, name.to_owned(), func)))
        })
        .collect::<Vec<_>>();
    let mut descriptions = Vec::with_capacity(describes.len());
    for (ty, name, describe) in describes {
        let (packed,) = describe.typed(&store)?.call(&mut store, ()).unwrap();
        let descr = memory.with_unpacked_slice(&mut store, packed, |slice| {
            Ok(match ty {
                DescrType::Table => ModuleItemDef::Entity(EntityDef::Table(TableDef::decode(&mut &slice[..])?)),
                DescrType::Tuple => ModuleItemDef::Tuple(TupleDef::decode(&mut &slice[..])?),
                DescrType::Reducer => ModuleItemDef::Entity(EntityDef::Reducer(ReducerDef::decode(&mut &slice[..])?)),
                DescrType::Repeater => {
                    ModuleItemDef::Entity(EntityDef::Repeater(RepeaterDef::decode(&mut &slice[..])?))
                }
            })
        })?;
        descriptions.push((name, descr));
    }
    Ok(descriptions)
}

struct WasmCtx {
    mem: Option<Memory>,
}

#[derive(Copy, Clone)]
struct Memory {
    mem: wasmtime::Memory,
    dealloc: TypedFunc<(u32, u32), ()>,
}
impl Memory {
    fn deref_slice<'a>(&self, store: &'a impl AsContext, offset: u32, len: u32) -> Option<&'a [u8]> {
        self.mem
            .data(store.as_context())
            .get(offset as usize..)?
            .get(..len as usize)
    }
    fn with_unpacked_slice<R>(
        &self,
        mut store: impl AsContextMut,
        packed: u64,
        f: impl FnOnce(&[u8]) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        let offset = (packed >> 32) as u32;
        let len = (packed & 0xFFFF_FFFF) as u32;
        let slice = self.deref_slice(&store, offset, len).context("invalid ptr")?;
        let res = f(slice);
        let dealloc_res = self
            .dealloc
            .call(store.as_context_mut(), (offset, len))
            .context("error while deallocating");
        if res.is_err() {
            res
        } else {
            dealloc_res.and(res)
        }
    }
}
