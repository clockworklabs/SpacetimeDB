use spacetimedb::db::{Column, Schema, SpacetimeDB, schema::ColType, transactional_db::Transaction};
use tokio::runtime::Builder;
use tokio::fs;
use std::{collections::HashMap, error::Error, sync::{Arc, Mutex}, usize};
use wasmer::{Bytes, CompilerConfig, Cranelift, Function, Instance, Memory, MemoryType, Module, Pages, Store, Universal, Value, imports, wasmparser::Operator, wat2wasm};
use wasmer_middlewares::{
    metering::get_remaining_points,
    Metering,
};
use wasmer::Extern;
use lazy_static::lazy_static;

lazy_static! {
    static ref STDB: Mutex<SpacetimeDB> = Mutex::new(SpacetimeDB::new());
    static ref TX: Mutex<Option<Transaction>> = Mutex::new(None);
    static ref INSTANCE: Mutex<Option<Instance>> = Mutex::new(None);
}

fn offset_to_string(offset: u32) -> String {
    let mut instance_mutex = INSTANCE.lock().unwrap();
    let instance = instance_mutex.as_mut().unwrap();
    let memory = instance.exports.get_memory("memory").unwrap();
    let view = memory.view::<u16>();
    let start = offset as usize / 2;
    let mut bytes = Vec::new();
    for c in view[start..].iter() {
        let v = c.get();
        if v == 0 {
            break;
        }
        bytes.push(v);
    }
    String::from_utf16(&bytes).unwrap()
}

fn console_log(message: u32) {
    println!("console: {}", offset_to_string(message));
}

fn abort(message: u32, file_name: u32, line: u32, column: u32) {
    println!("abort: {}", offset_to_string(message));
    println!("    file_name: {}", offset_to_string(file_name));
    println!("    line: {}", offset_to_string(line));
    println!("    column: {}", offset_to_string(column));
}

fn scan() {
    println!("SCANNING:");
    let stdb = STDB.lock().unwrap();
    let mut tx_mutex = TX.lock().unwrap();
    let tx = tx_mutex.as_mut().unwrap();
    for x in stdb.iter(tx, 0) {
        println!("{:?}", x);
    }
}

fn begin_tx() {
    let mut stdb = STDB.lock().unwrap();
    let tx = stdb.begin_tx();
    *TX.lock().unwrap() = Some(tx);
}

fn decode_schema(bytes: &mut &[u8]) -> Schema {
    let mut columns: Vec<Column> = Vec::new();
    while bytes.len() > 0 && bytes[0] != 0 {
        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[0..4]);
        println!("dst: {:?}", dst);
        *bytes = &bytes[4..];
        let col_type = ColType::from_u32(u32::from_le_bytes(dst));

        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[0..4]);
        println!("dst: {:?}", dst);
        *bytes = &bytes[4..];
        let col_id = u32::from_le_bytes(dst);

        columns.push(Column {
            col_type, 
            col_id,
        });
    }
    Schema {
        columns,
    }
}

fn read_output_bytes() -> Vec<u8> {
    let mut instance_mutex = INSTANCE.lock().unwrap();
    let instance = instance_mutex.as_mut().unwrap();
    let memory = instance.exports.get_memory("memory").unwrap();
    let view = memory.view::<u8>();
    view[0..256].iter().map(|c| c.get()).collect::<Vec<u8>>()
}

fn insert(table_id: u32) {
    let buffer = read_output_bytes();

    let mut stdb = STDB.lock().unwrap();
    let mut tx_mutex = TX.lock().unwrap();
    let tx = tx_mutex.as_mut().unwrap();

    let schema = SpacetimeDB::schema_for_table(&stdb.txdb, tx, table_id);
    let row = SpacetimeDB::decode_row(&schema, &buffer[..]);
    stdb.insert(tx, table_id, row);
}

fn create_table(table_id: u32) {
    let buffer = read_output_bytes();
    
    let mut stdb = STDB.lock().unwrap();
    let mut tx_mutex = TX.lock().unwrap();
    let tx = tx_mutex.as_mut().unwrap();

    let schema = decode_schema(&mut &buffer[..]);
    stdb.create_table(tx, table_id, schema).unwrap();
}

async fn async_main() -> Result<(), Box<dyn Error>> {
    let path = fs::canonicalize(format!("{}{}", env!("CARGO_MANIFEST_DIR"),"/wasm-test/build/debug.wat")).await.unwrap();
    let wat = fs::read(path).await?;
    // println!("{}", String::from_utf8(wat.to_owned()).unwrap());
    let wasm_bytes= wat2wasm(&wat)?;

    let cost_function = |operator: &Operator| -> u64 {
        match operator {
            Operator::LocalGet { .. } => 1,
            Operator::I32Const { .. } => 1,
            Operator::I32Add { .. } => 5,
            _ => 0,
        }
    };

    let metering = Arc::new(Metering::new(100000, cost_function));
    let mut compiler_config = Cranelift::default();
    compiler_config.push_middleware(metering);

    let store = Store::new(&Universal::new(compiler_config).engine());
    let module = Module::new(&store, wasm_bytes)?;
    let import_object = imports! {
        "stdb" => {
            "_insert" => Function::new_native(&store, insert),
            "_createTable" => Function::new_native(&store, create_table),
        },
        "env" => {
            "abort" => Function::new_native(&store, abort),
            "console.log" => Function::new_native(&store, console_log),
        }
    };

    let instance = Instance::new(&module, &import_object)?;
    println!("HAP");

    for (name, value) in instance.exports.iter() {
        println!("{:?} {:?}", name, value);
        match value {
            Extern::Function(f) => {},
            Extern::Global(_g) => {},
            Extern::Table(_t) => {},
            Extern::Memory(_m) => {},
        }
    }

    let reduce = instance.exports.get_function("reduce")?.native::<u64, ()>()?;
    *INSTANCE.lock().unwrap() = Some(instance);
    begin_tx();
    scan();

    reduce.call(34234)?;

    scan();

    Ok(())
}

fn main() {
    // Create a single threaded run loop
    Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
        .unwrap();
}