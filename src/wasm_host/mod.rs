use std::{error::Error, collections::HashMap, sync::{Mutex, Arc}};
use spacetimedb_bindings::{decode_schema, encode_schema, Schema};
use tokio::{spawn, sync::{mpsc, oneshot}};
use wasmer::{Store, Universal, Module, Instance, imports, Function, WasmerEnv, LazyInit, Memory, NativeFunc, ValType, wasmparser::Operator, CompilerConfig, WasmPtr, Array};
use wasmer_middlewares::{metering::{set_remaining_points, get_remaining_points, MeteringPoints}, Metering};
use crate::{hash::{Hash, hash_bytes}, db::{SpacetimeDB, transactional_db::Transaction}};
use lazy_static::lazy_static;

lazy_static! {
    static ref STDB: Mutex<SpacetimeDB> = Mutex::new(SpacetimeDB::new());

    // TODO: probably store these inside STDB
    static ref TX_MAP: Mutex<HashMap<u64, Transaction>> = Mutex::new(HashMap::new());
    static ref TX_ID: Mutex<u64> = Mutex::new(0);
}

#[derive(WasmerEnv, Clone, Default)]
pub struct ReducerEnv {
    tx_id: u64,
    #[wasmer(export)]
    memory: LazyInit<Memory>,
    #[wasmer(export(name = "reduce"))]
    reduce: LazyInit<NativeFunc<u64, ()>>,
    #[wasmer(export(name = "alloc"))]
    alloc: LazyInit<NativeFunc<u32, WasmPtr<u8, Array>>>,
}

fn c_str_to_string(memory: &Memory, ptr: u32) -> String {
    let view = memory.view::<u8>();
    let start = ptr as usize;
    let mut bytes = Vec::new();
    for c in view[start..].iter() {
        let v = c.get();
        if v == 0 {
            break;
        }
        bytes.push(v);
    }
    String::from_utf8(bytes).unwrap()
}

fn console_log(env: &ReducerEnv, level: u8, ptr: u32) {
    let memory = env.memory.get_ref().expect("Initialized memory");
    let s = c_str_to_string(memory, ptr);
    match level {
        0 => eprintln!("error: {}", s),
        1 => println!("warn: {}", s),
        2 => println!("info: {}", s),
        3 => println!("debug: {}", s),
        _ => println!("debug: {}", s),
    }
}

fn read_output_bytes(memory: &Memory, ptr: u32) -> Vec<u8> {
    let view = memory.view::<u8>();
    let start = ptr as usize;
    let end = ptr as usize + 256;
    view[start..end].iter().map(|c| c.get()).collect::<Vec<u8>>()
}

fn insert(env: &ReducerEnv, table_id: u32, ptr: u32) {
    let buffer = read_output_bytes(env.memory.get_ref().expect("Initialized memory"), ptr);

    let mut stdb = STDB.lock().unwrap();
    let mut tx_map = TX_MAP.lock().unwrap();
    let tx = tx_map.get_mut(&env.tx_id).unwrap();

    let schema = stdb.schema_for_table(tx, table_id).unwrap();
    let row = SpacetimeDB::decode_row(&schema, &buffer[..]);

    stdb.insert(tx, table_id, row);
}

fn create_table(env: &ReducerEnv, table_id: u32, ptr: u32) {
    let buffer = read_output_bytes(env.memory.get_ref().expect("Initialized memory"), ptr);

    let mut stdb = STDB.lock().unwrap();
    let mut tx_map = TX_MAP.lock().unwrap();
    let tx = tx_map.get_mut(&env.tx_id).unwrap();

    let schema = decode_schema(&mut &buffer[..]);
    stdb.create_table(tx, table_id, schema).unwrap();
}

fn iter(env: &ReducerEnv, table_id: u32) -> u64 {
    let stdb = STDB.lock().unwrap();
    let mut tx_map = TX_MAP.lock().unwrap();
    let tx = tx_map.get_mut(&env.tx_id).unwrap();

    let memory = env.memory.get_ref().expect("Initialized memory");

    let mut bytes = Vec::new();
    let schema = stdb.schema_for_table(tx, table_id).unwrap();
    encode_schema(Schema { columns: schema}, &mut bytes);

    for row in stdb.iter(tx, table_id).unwrap() {
        SpacetimeDB::encode_row(row, &mut bytes);
    }

    let alloc_func = env.alloc.get_ref().expect("Intialized alloc function");
    let ptr = alloc_func.call(bytes.len() as u32).unwrap();
    let values = ptr.deref(memory, 0, bytes.len() as u32).unwrap();

    for (i, byte) in bytes.iter().enumerate() {
        values[i].set(*byte);
    }

    let mut data = ptr.offset() as u64;
    data = data << 32 | bytes.len() as u64;
    println!("{:?}", data.to_be_bytes());
    return data;
}

fn get_remaining_points_value(instance: &Instance) -> u64 {
    let remaining_points = get_remaining_points(instance);
    let remaining_points = match remaining_points {
        MeteringPoints::Remaining(x) => x,
        MeteringPoints::Exhausted => 0,
    };
    return remaining_points;
}

#[derive(Debug)]
enum HostCommand {
    Add { wasm_bytes: Vec<u8>, respond_to: oneshot::Sender<Result<Hash, Box<dyn Error + Send + Sync>>> },
    Run { hash: Hash, respond_to: oneshot::Sender<Result<(), Box<dyn Error + Send + Sync>>> }
}

fn run(store: &Store, module: &Module, points: u64) -> Result<(), Box<dyn Error + Send + Sync>> {
    let tx_id = {
        let mut stdb = STDB.lock().unwrap();
        let tx = stdb.begin_tx();
        let id = *TX_ID.lock().unwrap();
        *TX_ID.lock().unwrap() += 1;
        TX_MAP.lock().unwrap().insert(id, tx);
        id
    };
    let import_object = imports! {
        "env" => {
            "_insert" => Function::new_native_with_env(
                &store,
                ReducerEnv { tx_id, ..Default::default()},
                insert,
            ),
            "_create_table" => Function::new_native_with_env(
                &store,
                ReducerEnv { tx_id, ..Default::default()},
                create_table,
            ),
            "_iter" => Function::new_native_with_env(
                &store,
                ReducerEnv { tx_id, ..Default::default()},
                iter
            ),
            "_console_log" => Function::new_native_with_env(
                &store,
                ReducerEnv { tx_id, ..Default::default()},
                console_log
            ),
        }
    };
    let instance = Instance::new(&module, &import_object)?;
    set_remaining_points(&instance, points);

    // Init if available
    let init = instance.exports.get_native_function::<(), ()>("_init");
    if let Some(init) = init.ok() {
        let _ = init.call();
    }

    let reduce = instance.exports.get_function("reduce")?.native::<u64, ()>()?;

    let start = std::time::Instant::now();
    println!("Running Wasm reduce...");
    let result = reduce.call(0);
    let duration = start.elapsed();
    let remaining_points = get_remaining_points_value(&instance);
    println!("Wasm reduce: time {} us, gas used {}", duration.as_micros(), 1_000_000 - remaining_points);
    println!();

    if let Some(err) = result.err() {
        let mut stdb = STDB.lock().unwrap();
        let mut tx_map = TX_MAP.lock().unwrap();
        let tx = tx_map.remove(&tx_id).unwrap();
        stdb.rollback_tx(tx);

        let e = &err;
        let frames = e.trace();
        let frames_len = frames.len();

        println!("Runtime error:");
        for i in 0..frames_len {
            println!(
                "  Frame #{}: {:?}::{:?}",
                frames_len - i,
                frames[i].module_name(),
                frames[i].function_name().or(Some("<func>")).unwrap()
            );
        }
    } else {
        let mut stdb = STDB.lock().unwrap();
        let mut tx_map = TX_MAP.lock().unwrap();
        let tx = tx_map.remove(&tx_id).unwrap();
        stdb.commit_tx(tx);
    }
    Ok(())
}

fn add(modules: &mut HashMap<Hash, Module>, store: &Store, wasm_bytes: impl AsRef<[u8]>) -> Result<Hash, Box<dyn Error + Send + Sync>> {
    let hash = hash_bytes(&wasm_bytes);
    let module = Module::new(store, wasm_bytes)?;
    let mut found = false;
    for f in module.exports().functions() {
        if f.name() != "reduce" {
            continue;
        }
        found = true;
        let ty = f.ty();
        if ty.params().len() != 1 {
            return Err("Reduce function has wrong number of params.".into());
        }
        if ty.params()[0] != ValType::I64 {
            return Err("Incorrect param type for reducer.".into());
        }
    }
    if !found {
        return Err("Reduce function not found in module.".into());
    }
    modules.insert(hash, module);
    Ok(hash)
}

pub struct Host {
    tx: mpsc::Sender<HostCommand>
}

impl Host {
    pub fn new() -> Self {
        let (tx, mut rx) = mpsc::channel::<HostCommand>(1024);
        spawn(async move {
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
            compiler_config.push_middleware(metering);

            let store = Store::new(&Universal::new(compiler_config).engine());
            let mut reducers: HashMap<Hash, Module> = HashMap::new();

            while let Some(command) = rx.recv().await {
                match command {
                    HostCommand::Add { wasm_bytes, respond_to } => {
                        let res = add(&mut reducers, &store, wasm_bytes);
                        respond_to.send(res).unwrap();
                    }
                    HostCommand::Run { hash, respond_to } => {
                        let module = reducers.get(&hash);
                        if let Some(module) = module {
                            let res = run(&store, module, 1_000_000);
                            respond_to.send(res).unwrap();
                        }
                    },
                }
            }
        });
        Self {
            tx
        }
    }

    pub async fn add_reducer(&self, wasm_bytes: Vec<u8>) -> Result<Hash, Box<dyn Error + Send + Sync>> {
        let (tx, rx) = oneshot::channel::<Result<Hash, Box<dyn Error + Send + Sync>>>();
        self.tx.send(HostCommand::Add { wasm_bytes, respond_to: tx }).await.unwrap();
        rx.await.unwrap()
    }

    pub async fn run_reducer(&self, hash: Hash) -> Result<(), Box<dyn Error + Send + Sync>>  {
        let (tx, rx) = oneshot::channel::<Result<(), Box<dyn Error + Send + Sync>>>();
        self.tx.send(HostCommand::Run { hash, respond_to: tx }).await.unwrap();
        rx.await.unwrap()
    }
}

// async fn async_main() -> Result<(), Box<dyn Error>> {
//     let path = fs::canonicalize(format!("{}{}", env!("CARGO_MANIFEST_DIR"),"/rust-wasm-test/wat")).await.unwrap();
//     let wat = fs::read(path).await?;
//     // println!("{}", String::from_utf8(wat.to_owned()).unwrap());
//     let wasm_bytes= wat2wasm(&wat)?;

//     let cost_function = |operator: &Operator| -> u64 {
//         match operator {
//             Operator::LocalGet { .. } => 1,
//             Operator::I32Const { .. } => 1,
//             Operator::I32Add { .. } => 1,
//             _ => 1,
//         }
//     };

//     let initial_points = 1000000;
//     let metering = Arc::new(Metering::new(initial_points, cost_function));
//     let mut compiler_config = wasmer_compiler_llvm::LLVM::default();
//     compiler_config.opt_level(wasmer_compiler_llvm::LLVMOptLevel::Aggressive);
//     // let mut compiler_config = Cranelift::default();
//     // compiler_config.opt_level(wasmer::CraneliftOptLevel::Speed);
//     compiler_config.push_middleware(metering);

//     let store = Store::new(&Universal::new(compiler_config).engine());
//     let module = Module::new(&store, wasm_bytes)?;
//     let import_object = imports! {
//         "env" => {
//             "_insert" => Function::new_native(&store, insert),
//             "_create_table" => Function::new_native(&store, create_table),
//             "_iter_next" => Function::new_native(&store, iter_next),
//             "abort" => Function::new_native(&store, abort),
//             "console.log" => Function::new_native(&store, console_log),
//         }
//     };

//     let instance = Instance::new(&module, &import_object)?;

//     let warmup = instance.exports.get_function("warmup")?.native::<(), ()>()?;
//     let reduce = instance.exports.get_function("reduce")?.native::<u64, ()>()?;
//     *INSTANCE.lock().unwrap() = Some(instance);

//     println!("Running warmup...");
//     let start = std::time::Instant::now();
//     warmup.call()?;
//     let duration = start.elapsed();
//     let remaining_points = get_remaining_points_value();
//     println!("warmup: time {} us, gas used {}", duration.as_micros(), initial_points - remaining_points);
//     println!();

//     begin_tx();
//     scan();

//     let start = std::time::Instant::now();
//     println!("Running Wasm reduce...");
//     reduce.call(34234)?;
//     let duration = start.elapsed();
//     let remaining_points = get_remaining_points_value();
//     println!("Wasm reduce: time {} us, gas used {}", duration.as_micros(), initial_points - remaining_points);
//     println!();

//     scan();

//     rollback_tx();
    
//     scan();
    
//     let start = std::time::Instant::now();
//     println!("Running Wasm reduce...");
//     reduce.call(34234)?;
//     let duration = start.elapsed();
//     let remaining_points = get_remaining_points_value();
//     println!("Wasm reduce: time {} us, gas used {}", duration.as_micros(), initial_points - remaining_points);
//     println!();

//     scan();
    
//     rollback_tx();
   
//     let start = std::time::Instant::now();
//     println!("Running Rust reduce...");
//     rust_reduce();
//     let duration = start.elapsed();
//     println!("Rust reduce: time {} us", duration.as_micros());
//     println!();
    
//     commit_tx();

//     scan();

//     Ok(())
// }