use std::fs;
use std::path::{Path, PathBuf};

use clap::Arg;
use convert_case::{Case, Casing};
use spacetimedb_lib::sats::{AlgebraicType, AlgebraicTypeRef, Typespace};
use spacetimedb_lib::{bsatn, EntityDef, ModuleItemDef};
use wasmtime::{AsContext, Caller, ExternType, Trap};

mod code_indenter;
pub mod csharp;

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
            Arg::new("lang")
                .required(true)
                .long("lang")
                .short('l')
                .value_parser(clap::value_parser!(Language))
                .help("The language to generate"),
        )
        .after_help("Run `spacetime help publish` for more detailed information.")
}

pub fn exec(args: &clap::ArgMatches) -> anyhow::Result<()> {
    let project_path = args.get_one::<PathBuf>("project_path").unwrap();
    let wasm_file = args.get_one::<PathBuf>("wasm_file").cloned();
    let out_dir = args.get_one::<PathBuf>("out_dir").unwrap();
    let lang = *args.get_one::<Language>("lang").unwrap();

    let wasm_file = match wasm_file {
        Some(x) => x,
        None => crate::tasks::build(project_path)?,
    };

    if !out_dir.exists() {
        return Err(anyhow::anyhow!(
            "Output directory '{}' does not exist. Please create the directory and rerun this command.",
            out_dir.to_str().unwrap()
        ));
    }

    for (fname, code) in generate(&wasm_file, lang)? {
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
    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            Self::Csharp => Some(clap::builder::PossibleValue::new("csharp").aliases(["c#", "cs"])),
        }
    }
}

pub struct GenCtx {
    typespace: Typespace,
    names: Vec<Option<String>>,
}

pub fn generate(wasm_file: &Path, lang: Language) -> anyhow::Result<impl Iterator<Item = (String, String)>> {
    let Language::Csharp = lang;
    let (typespace, descriptions) = extract_descriptions(wasm_file)?;
    let mut names = vec![None; typespace.types.len()];
    for (name, descr) in &descriptions {
        let typeref = match descr {
            ModuleItemDef::Entity(EntityDef::Table(t)) => t.data,
            ModuleItemDef::Entity(EntityDef::Reducer(_)) => continue,
            ModuleItemDef::TypeAlias(r) => *r,
        };
        names[typeref.idx()] = Some(name.clone())
    }
    let ctx = GenCtx { typespace, names };
    Ok(descriptions.into_iter().filter_map(move |(name, desc)| match desc {
        ModuleItemDef::Entity(EntityDef::Table(table)) => {
            let code = csharp::autogen_csharp_table(&ctx, &name, &table);
            Some((name + ".cs", code))
        }
        ModuleItemDef::TypeAlias(r) => match &ctx.typespace[r] {
            AlgebraicType::Sum(_) => todo!(),
            AlgebraicType::Product(prod) => {
                let code = csharp::autogen_csharp_tuple(&ctx, &name, prod);
                Some((name + ".cs", code))
            }
            AlgebraicType::Builtin(_) => todo!(),
            AlgebraicType::Ref(_) => todo!(),
        },
        // I'm not sure exactly how this should work; when does init_database get called with csharp?
        ModuleItemDef::Entity(EntityDef::Reducer(reducer)) if reducer.name.as_deref() == Some("__init__") => None,
        ModuleItemDef::Entity(EntityDef::Reducer(reducer)) => {
            let code = csharp::autogen_csharp_reducer(&ctx, &reducer);
            let pascalcase = name.to_case(Case::Pascal);
            Some((pascalcase + "Reducer.cs", code))
        }
    }))
}

fn extract_descriptions(wasm_file: &Path) -> anyhow::Result<(Typespace, Vec<(String, ModuleItemDef)>)> {
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
        "spacetime",
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
    linker.func_wrap("spacetime", "_buffer_alloc", WasmCtx::buffer_alloc)?;
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
    // let alloc: TypedFunc<(u32,), (u32,)> = instance.get_func(&mut store, "alloc").unwrap().typed(&store).unwrap();
    enum DescrType {
        Table,
        TypeAlias,
        Reducer,
    }
    let typespace = match instance.get_func(&mut store, "__describe_typespace__") {
        Some(f) => {
            let buf: u32 = f.typed(&store)?.call(&mut store, ()).unwrap();
            let slice = store.data_mut().buffers.remove(buf as usize);
            bsatn::from_reader(&mut &slice[..])?
        }
        None => Typespace::default(),
    };
    let describes = instance
        .exports(&mut store)
        .filter_map(|exp| {
            let sym = exp.name();
            let func = exp.into_func()?;
            let prefixes = [
                ("__describe_table__", DescrType::Table),
                ("__describe_type_alias__", DescrType::TypeAlias),
                ("__describe_reducer__", DescrType::Reducer),
            ];
            prefixes
                .into_iter()
                .find_map(|(prefix, ty)| sym.strip_prefix(prefix).map(|name| (ty, name.to_owned(), func)))
        })
        .collect::<Vec<_>>();
    let mut descriptions = Vec::with_capacity(describes.len());
    for (ty, name, describe) in describes {
        let (val,): (u32,) = describe.typed(&store)?.call(&mut store, ()).unwrap();
        let mut slice = || store.data_mut().buffers.remove(val as usize);
        let descr = match ty {
            DescrType::Table => ModuleItemDef::Entity(EntityDef::Table(bsatn::from_reader(&mut &slice()[..])?)),
            DescrType::TypeAlias => ModuleItemDef::TypeAlias(AlgebraicTypeRef(val)),
            DescrType::Reducer => ModuleItemDef::Entity(EntityDef::Reducer(bsatn::from_reader(&mut &slice()[..])?)),
        };
        descriptions.push((name, descr));
    }
    Ok((typespace, descriptions))
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
