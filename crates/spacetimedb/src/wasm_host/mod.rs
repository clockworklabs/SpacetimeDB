use crate::{
    db::{transactional_db::Transaction, SpacetimeDB},
    hash::{hash_bytes, Hash},
};
use anyhow;
use lazy_static::lazy_static;
use spacetimedb_bindings::{decode_schema, encode_schema, Schema};
use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, oneshot};
use wasmer::{
    imports, wasmparser::Operator, Array, CompilerConfig, Function, Instance, LazyInit, Memory, Module, NativeFunc,
    Store, Universal, ValType, WasmPtr, WasmerEnv,
};
use wasmer_middlewares::{
    metering::{get_remaining_points, set_remaining_points, MeteringPoints},
    Metering,
};

lazy_static! {
    pub static ref HOST: Mutex<Host> = Mutex::new(HostActor::spawn());
    static ref STDB: Mutex<SpacetimeDB> = Mutex::new(SpacetimeDB::new());

    // TODO: probably store these inside STDB
    static ref TX_MAP: Mutex<HashMap<u64, Transaction>> = Mutex::new(HashMap::new());
    static ref TX_ID: Mutex<u64> = Mutex::new(0);
}

pub fn get_host() -> Host {
    HOST.lock().unwrap().clone()
}

#[derive(WasmerEnv, Clone, Default)]
pub struct ReducerEnv {
    tx_id: u64,
    #[wasmer(export)]
    memory: LazyInit<Memory>,
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
    encode_schema(Schema { columns: schema }, &mut bytes);

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
    Add {
        wasm_bytes: Vec<u8>,
        respond_to: oneshot::Sender<Result<Hash, anyhow::Error>>,
    },
    Call {
        hash: Hash,
        reducer_name: String,
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
    },
}

struct HostActor {
    store: Store,
    modules: HashMap<Hash, Module>,
}

impl HostActor {
    pub fn spawn() -> Host {
        let (tx, mut rx) = mpsc::channel(8);
        tokio::spawn(async move {
            let mut actor = HostActor::new();
            while let Some(command) = rx.recv().await {
                actor.handle_message(command);
            }
        });
        Host { tx }
    }

    fn new() -> Self {
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
        let modules: HashMap<Hash, Module> = HashMap::new();

        Self { store, modules }
    }

    fn handle_message(&mut self, message: HostCommand) {
        match message {
            HostCommand::Add { wasm_bytes, respond_to } => {
                respond_to.send(self.add(wasm_bytes)).unwrap();
            }
            HostCommand::Call {
                hash,
                reducer_name,
                respond_to,
            } => {
                respond_to.send(self.run(hash, reducer_name)).unwrap();
            }
        }
    }

    fn add(&mut self, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let hash = hash_bytes(&wasm_bytes);
        let module = Module::new(&self.store, wasm_bytes)?;
        let mut found = false;
        for f in module.exports().functions() {
            if !f.name().starts_with("_reducer_") {
                continue;
            }
            found = true;
            let ty = f.ty();
            if ty.params().len() != 1 {
                return Err(anyhow::anyhow!("Reduce function has wrong number of params."));
            }
            if ty.params()[0] != ValType::I64 {
                return Err(anyhow::anyhow!("Incorrect param type for reducer."));
            }
        }
        if !found {
            return Err(anyhow::anyhow!("Reduce function not found in module."));
        }
        self.modules.insert(hash, module);
        Ok(hash)
    }

    fn run(&mut self, hash: Hash, reducer_name: String) -> Result<(), anyhow::Error> {
        let tx_id = {
            let mut stdb = STDB.lock().unwrap();
            let tx = stdb.begin_tx();
            let id = *TX_ID.lock().unwrap();
            *TX_ID.lock().unwrap() += 1;
            TX_MAP.lock().unwrap().insert(id, tx);
            id
        };

        let module = self.modules.get(&hash);
        let module = match module {
            Some(x) => x,
            None => return Err(anyhow::anyhow!("No such module.")),
        };

        let import_object = imports! {
            "env" => {
                "_insert" => Function::new_native_with_env(
                    &self.store,
                    ReducerEnv { tx_id, ..Default::default()},
                    insert,
                ),
                "_create_table" => Function::new_native_with_env(
                    &self.store,
                    ReducerEnv { tx_id, ..Default::default()},
                    create_table,
                ),
                "_iter" => Function::new_native_with_env(
                    &self.store,
                    ReducerEnv { tx_id, ..Default::default()},
                    iter
                ),
                "_console_log" => Function::new_native_with_env(
                    &self.store,
                    ReducerEnv { tx_id, ..Default::default()},
                    console_log
                ),
            }
        };

        let points = 1_000_000;
        println!("HAP");
        let instance = Instance::new(&module, &import_object)?;
        println!("HAP2");
        set_remaining_points(&instance, points);

        // Init if available
        let init = instance.exports.get_native_function::<(), ()>("_init");
        if let Some(init) = init.ok() {
            let _ = init.call();
        }

        let reducer_name = format!("_reducer_{}", reducer_name);
        let reduce = instance.exports.get_function(&reducer_name)?.native::<u64, ()>()?;

        let start = std::time::Instant::now();
        println!("Running Wasm reduce...");
        let result = reduce.call(0);
        let duration = start.elapsed();
        let remaining_points = get_remaining_points_value(&instance);
        println!(
            "Wasm reduce: time {} us, gas used {}",
            duration.as_micros(),
            1_000_000 - remaining_points
        );
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
}

#[derive(Clone)]
pub struct Host {
    tx: mpsc::Sender<HostCommand>,
}

impl Host {
    pub async fn init_module(&self, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<Hash, anyhow::Error>>();
        self.tx
            .send(HostCommand::Add {
                wasm_bytes,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }

    pub async fn call_reducer(&self, hash: Hash, reducer_name: String) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx
            .send(HostCommand::Call {
                hash,
                reducer_name,
                respond_to: tx,
            })
            .await?;
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
