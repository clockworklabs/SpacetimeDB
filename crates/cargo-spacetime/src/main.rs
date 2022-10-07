use std::collections::HashMap;

use spacetimedb_lib::type_def::resolve_refs::TypeRef;
use spacetimedb_lib::TupleDef;
use wasmtime::{ExternType, Trap, TypedFunc};

mod code_indenter;
pub mod csharp;

const INDENT: &str = "\t";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::from_file(&engine, "foo.wasm")?;
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
    let describes = instance
        .exports(&mut store)
        .filter_map(|exp| {
            let name = exp.name();
            name.strip_prefix("__describe_table_refs__")
                .or_else(|| name.strip_prefix("__describe_tuple_refs__"))
                .map(|table_name| (table_name.to_owned(), exp.into_func().unwrap()))
        })
        .collect::<Vec<_>>();
    let mut tables = HashMap::new();
    for (table_name, describe) in describes {
        let describe: TypedFunc<(), (u64,)> = describe.typed(&store).unwrap();
        let (val,) = describe.call(&mut store, ()).unwrap();
        let offset = ((val & 0xFFFF_FFFF_0000_0000) >> 32) as u32;
        let len = (val & 0x0000_0000_FFFF_FFFF) as u32;
        let slice = &memory.data(&store)[offset as usize..][..len as usize];
        let table_def = TupleDef::<TypeRef>::decode(&slice).0.unwrap();
        dealloc.call(&mut store, (offset, len)).unwrap();
        tables.insert(table_name, table_def);
    }
    for (k, v) in &tables {
        println!("{}", csharp::autogen_csharp_tuple(k, v, None, &[]));
    }
    Ok(())
}
