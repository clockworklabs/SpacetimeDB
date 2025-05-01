use std::mem;
use std::path::Path;

use anyhow::Context;
use spacetimedb_lib::{bsatn, RawModuleDefV8};
use spacetimedb_lib::{RawModuleDef, MODULE_ABI_MAJOR_VERSION};
use spacetimedb_primitives::errno;
use spacetimedb_schema::def::{ModuleDef, ReducerDef, ScopedTypeName, TableDef, TypeDef};
use spacetimedb_schema::identifier::Identifier;
use wasmtime::{Caller, StoreContextMut};

mod code_indenter;
pub mod csharp;
pub mod rust;
pub mod typescript;
mod util;

pub use self::csharp::Csharp;
pub use self::rust::Rust;
pub use self::typescript::TypeScript;
pub use util::AUTO_GENERATED_PREFIX;

pub fn generate(module: &ModuleDef, lang: &dyn Lang) -> Vec<(String, String)> {
    itertools::chain!(
        module
            .tables()
            .map(|tbl| (lang.table_filename(module, tbl), lang.generate_table(module, tbl))),
        module
            .types()
            .map(|typ| (lang.type_filename(&typ.name), lang.generate_type(module, typ))),
        util::iter_reducers(module).map(|reducer| {
            (
                lang.reducer_filename(&reducer.name),
                lang.generate_reducer(module, reducer),
            )
        }),
        lang.generate_globals(module),
    )
    .collect()
}

pub trait Lang {
    fn table_filename(&self, module: &ModuleDef, table: &TableDef) -> String;
    fn type_filename(&self, type_name: &ScopedTypeName) -> String;
    fn reducer_filename(&self, reducer_name: &Identifier) -> String;

    fn generate_table(&self, module: &ModuleDef, tbl: &TableDef) -> String;
    fn generate_type(&self, module: &ModuleDef, typ: &TypeDef) -> String;
    fn generate_reducer(&self, module: &ModuleDef, reducer: &ReducerDef) -> String;
    fn generate_globals(&self, module: &ModuleDef) -> Vec<(String, String)>;
}

pub fn extract_descriptions(wasm_file: &Path) -> anyhow::Result<RawModuleDef> {
    let module = compile_wasm(wasm_file)?;
    extract_descriptions_from_module(module)
}

pub fn compile_wasm(wasm_file: &Path) -> anyhow::Result<wasmtime::Module> {
    wasmtime::Module::from_file(&wasmtime::Engine::default(), wasm_file)
}

#[allow(clippy::disallowed_macros)]
pub fn extract_descriptions_from_module(module: wasmtime::Module) -> anyhow::Result<RawModuleDef> {
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
            let slice = deref_slice(mem, message_ptr, message_len).unwrap();
            println!("from wasm: {}", String::from_utf8_lossy(slice));
        },
    )?;
    linker.func_wrap(module_name, "bytes_sink_write", WasmCtx::bytes_sink_write)?;
    let instance = linker.instantiate(&mut store, &module)?;
    let memory = instance.get_memory(&mut store, "memory").context("no memory export")?;
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
    mem: Option<wasmtime::Memory>,
    sink: Vec<u8>,
}

fn deref_slice(mem: &[u8], offset: u32, len: u32) -> anyhow::Result<&[u8]> {
    anyhow::ensure!(offset != 0, "ptr is null");
    mem.get(offset as usize..)
        .and_then(|s| s.get(..len as usize))
        .context("pointer out of bounds")
}

fn read_u32(mem: &[u8], offset: u32) -> anyhow::Result<u32> {
    Ok(u32::from_le_bytes(deref_slice(mem, offset, 4)?.try_into().unwrap()))
}

impl WasmCtx {
    pub fn get_mem(&self) -> wasmtime::Memory {
        self.mem.expect("Initialized memory")
    }

    fn mem_env<'a>(ctx: impl Into<StoreContextMut<'a, Self>>) -> (&'a mut [u8], &'a mut Self) {
        let ctx = ctx.into();
        let mem = ctx.data().get_mem();
        mem.data_and_store_mut(ctx)
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
        let buffer_len = read_u32(mem, buffer_len_ptr)?;
        // Write `buffer` to `sink`.
        let buffer = deref_slice(mem, buffer_ptr, buffer_len)?;
        env.sink.extend(buffer);

        Ok(0)
    }
}
