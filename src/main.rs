use spacetimedb::{db::{Column, ColValue, Schema, SpacetimeDB, ColType, transactional_db::Transaction}, wasm_host::Host};
use tokio::runtime::Builder;
use tokio::fs;
use std::{error::Error, sync::{Arc, Mutex}, usize};
use wasmer::{CompilerConfig, Function, Instance, Module, Store, Universal, imports, wasmparser::Operator, wat2wasm};
use wasmer_middlewares::{Metering, metering::{MeteringPoints, get_remaining_points}};
use lazy_static::lazy_static;
use wasmer_compiler_llvm;
//use wasmer::Cranelift;

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
    println!("stdb=# select * from 0;");

    let stdb = STDB.lock().unwrap();
    let mut tx_mutex = TX.lock().unwrap();
    let tx = tx_mutex.as_mut().unwrap();

    let rows = stdb.iter(tx, 0);
    if let Some(rows) = rows {
        let rows: Vec<Vec<ColValue>> = rows.collect();
        let schema = stdb.schema_for_table(tx, 0).unwrap();
        for (i, c) in schema.iter().enumerate() {
            print!("  {} ({:?})  ", c.col_id, c.col_type);
            if i != schema.len() - 1 {
                print!("|");
            } 
        }
        println!();
        for (i, _) in schema.iter().enumerate() {
            print!("-----------");
            if i != schema.len() - 1 {
                print!("+");
            } 
        }
        println!();
        for r in &rows {
            for (i, v) in r.iter().enumerate() {
                print!("    {}     ", v);
                if i != r.len() - 1 {
                    print!("|");
                } 
            }
            println!();
        }
        println!("{} row(s)", rows.len());
    } else {
        println!("ERROR: Table \"0\" does not exist.");
    }
    println!();
}

fn begin_tx() {
    let mut stdb = STDB.lock().unwrap();
    let tx = stdb.begin_tx();
    *TX.lock().unwrap() = Some(tx);
}

fn rollback_tx() {
    {
        // TODO: need to actually roll this back because begin_tx does hold open commits
        let tx = TX.lock().unwrap().take().unwrap();
        let mut stdb = STDB.lock().unwrap();
        stdb.rollback_tx(tx);
    }

    let mut stdb = STDB.lock().unwrap();
    let tx = stdb.begin_tx();
    *TX.lock().unwrap() = Some(tx);
}

fn commit_tx() {
    {
        let tx = TX.lock().unwrap().take().unwrap();
        let mut stdb = STDB.lock().unwrap();
        stdb.commit_tx(tx);
    }

    let mut stdb = STDB.lock().unwrap();
    let tx = stdb.begin_tx();
    *TX.lock().unwrap() = Some(tx);
}

fn decode_schema(bytes: &mut &[u8]) -> Schema {
    let mut columns: Vec<Column> = Vec::new();
    while bytes.len() > 0 && bytes[0] != 0 {
        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[0..4]);
        *bytes = &bytes[4..];
        let col_type = ColType::from_u32(u32::from_le_bytes(dst));

        let mut dst = [0u8; 4];
        dst.copy_from_slice(&bytes[0..4]);
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

fn read_output_bytes(ptr: u32) -> Vec<u8> {
    let mut instance_mutex = INSTANCE.lock().unwrap();
    let instance = instance_mutex.as_mut().unwrap();
    let memory = instance.exports.get_memory("memory").unwrap();
    let view = memory.view::<u8>();
    let start = ptr as usize;
    let end = ptr as usize + 256;
    view[start..end].iter().map(|c| c.get()).collect::<Vec<u8>>()
}

fn insert(table_id: u32, ptr: u32) {
    let buffer = read_output_bytes(ptr);

    let mut stdb = STDB.lock().unwrap();
    let mut tx_mutex = TX.lock().unwrap();
    let tx = tx_mutex.as_mut().unwrap();

    let schema = stdb.schema_for_table(tx, table_id).unwrap();
    let row = SpacetimeDB::decode_row(&schema, &buffer[..]);
    stdb.insert(tx, table_id, row);
}

fn create_table(table_id: u32, ptr: u32) {
    let buffer = read_output_bytes(ptr);
    
    let mut stdb = STDB.lock().unwrap();
    let mut tx_mutex = TX.lock().unwrap();
    let tx = tx_mutex.as_mut().unwrap();

    let schema = decode_schema(&mut &buffer[..]);
    stdb.create_table(tx, table_id, schema).unwrap();
}

fn iter_next(table_id: u32, ptr: u32) {
    // let mut stdb = STDB.lock().unwrap();
    // let mut tx_mutex = TX.lock().unwrap();
    // let tx = tx_mutex.as_mut().unwrap();

    // let schema = decode_schema(&mut &buffer[..]);
    // stdb.create_table(tx, table_id, schema).unwrap();
}

fn get_remaining_points_value() -> u64 {
    let inst = &INSTANCE.lock().unwrap();
    let remaining_points = get_remaining_points(inst.as_ref().unwrap());
    let remaining_points = match remaining_points {
        MeteringPoints::Remaining(x) => x,
        MeteringPoints::Exhausted => 0,
    };
    return remaining_points;
}

fn rust_reduce() {
    let mut stdb = STDB.lock().unwrap();
    let mut tx_mutex = TX.lock().unwrap();
    let tx = tx_mutex.as_mut().unwrap();
    
    stdb.create_table(tx, 0, Schema { columns: vec![
        Column { col_id: 0, col_type: ColType::U32 },
        Column { col_id: 1, col_type: ColType::U32 },
        Column { col_id: 2, col_type: ColType::U32 },
    ] }).unwrap();

    for i in 0..100 {
        stdb.insert(tx, 0, vec![ColValue::U32(i), ColValue::U32(87), ColValue::U32(33)]);
    }
}

async fn async_main() -> Result<(), Box<dyn Error>> {
    let path = fs::canonicalize(format!("{}{}", env!("CARGO_MANIFEST_DIR"),"/rust-wasm-test/wat")).await.unwrap();
    let wat = fs::read(path).await?;
    // println!("{}", String::from_utf8(wat.to_owned()).unwrap());
    let wasm_bytes= wat2wasm(&wat)?;

    let cost_function = |operator: &Operator| -> u64 {
        match operator {
            Operator::LocalGet { .. } => 1,
            Operator::I32Const { .. } => 1,
            Operator::I32Add { .. } => 1,
            _ => 1,
        }
    };

    let initial_points = 1000000;
    let metering = Arc::new(Metering::new(initial_points, cost_function));
    let mut compiler_config = wasmer_compiler_llvm::LLVM::default();
    compiler_config.opt_level(wasmer_compiler_llvm::LLVMOptLevel::Aggressive);
    // let mut compiler_config = Cranelift::default();
    // compiler_config.opt_level(wasmer::CraneliftOptLevel::Speed);
    compiler_config.push_middleware(metering);

    let store = Store::new(&Universal::new(compiler_config).engine());
    let module = Module::new(&store, wasm_bytes)?;
    let import_object = imports! {
        "env" => {
            "_insert" => Function::new_native(&store, insert),
            "_create_table" => Function::new_native(&store, create_table),
            "_iter_next" => Function::new_native(&store, iter_next),
            "abort" => Function::new_native(&store, abort),
            "console.log" => Function::new_native(&store, console_log),
        }
    };

    let instance = Instance::new(&module, &import_object)?;

    let warmup = instance.exports.get_function("warmup")?.native::<(), ()>()?;
    let reduce = instance.exports.get_function("reduce")?.native::<u64, ()>()?;
    *INSTANCE.lock().unwrap() = Some(instance);

    println!("Running warmup...");
    let start = std::time::Instant::now();
    warmup.call()?;
    let duration = start.elapsed();
    let remaining_points = get_remaining_points_value();
    println!("warmup: time {} us, gas used {}", duration.as_micros(), initial_points - remaining_points);
    println!();

    begin_tx();
    scan();

    let start = std::time::Instant::now();
    println!("Running Wasm reduce...");
    reduce.call(34234)?;
    let duration = start.elapsed();
    let remaining_points = get_remaining_points_value();
    println!("Wasm reduce: time {} us, gas used {}", duration.as_micros(), initial_points - remaining_points);
    println!();

    scan();

    rollback_tx();
    
    scan();
    
    let start = std::time::Instant::now();
    println!("Running Wasm reduce...");
    reduce.call(34234)?;
    let duration = start.elapsed();
    let remaining_points = get_remaining_points_value();
    println!("Wasm reduce: time {} us, gas used {}", duration.as_micros(), initial_points - remaining_points);
    println!();

    scan();
    
    rollback_tx();
   
    let start = std::time::Instant::now();
    println!("Running Rust reduce...");
    rust_reduce();
    let duration = start.elapsed();
    println!("Rust reduce: time {} us", duration.as_micros());
    println!();
    
    commit_tx();

    scan();

    Ok(())
}

async fn async_main2() -> Result<(), Box<dyn Error + Send + Sync>> {
    let path = fs::canonicalize(format!("{}{}", env!("CARGO_MANIFEST_DIR"),"/rust-wasm-test/wat")).await.unwrap();
    let wat = fs::read(path).await?;
    // println!("{}", String::from_utf8(wat.to_owned()).unwrap());
    let wasm_bytes = wat2wasm(&wat)?.to_vec();
    let host = Host::new();
    let reducer = host.add_reducer(wasm_bytes).await?;
    host.run_reducer(reducer).await?;
    //host.run_reducer(reducer).await?;
    Ok(())
}

fn main() {
    // Create a single threaded run loop
    Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main2())
        .unwrap();
}

