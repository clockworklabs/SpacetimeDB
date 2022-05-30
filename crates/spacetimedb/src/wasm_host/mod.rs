mod instance_env;
mod module_host;

use crate::hash::{hash_bytes, Hash};
use anyhow;
use lazy_static::lazy_static;
use log;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, oneshot};
use wasmer::{wasmparser::Operator, CompilerConfig, Module, Store, Universal, ValType};
use wasmer_middlewares::Metering;

use self::module_host::ModuleHost;

const REDUCE_DUNDER: &str = "__reducer__";
const _MIGRATE_DATABASE_DUNDER: &str = "__migrate_database__";

lazy_static! {
    pub static ref HOST: Mutex<Host> = Mutex::new(Host::spawn());
}

pub fn get_host() -> Host {
    HOST.lock().unwrap().clone()
}

#[derive(Debug)]
enum HostCommand {
    InitModule {
        wasm_bytes: Vec<u8>,
        respond_to: oneshot::Sender<Result<Hash, anyhow::Error>>,
    },
    AddModule {
        wasm_bytes: Vec<u8>,
        respond_to: oneshot::Sender<Result<Hash, anyhow::Error>>,
    },
    CallReducer {
        hash: Hash,
        reducer_name: String,
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
    },
}

struct HostActor {
    store: Store,
    modules: HashMap<Hash, ModuleHost>,
}

impl HostActor {
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
        let modules: HashMap<Hash, ModuleHost> = HashMap::new();

        Self { store, modules }
    }

    async fn handle_message(&mut self, message: HostCommand) {
        match message {
            HostCommand::InitModule { wasm_bytes, respond_to } => {
                respond_to.send(self.init_module(wasm_bytes).await).unwrap();
            }
            HostCommand::AddModule { wasm_bytes, respond_to } => {
                respond_to.send(self.add_module(wasm_bytes)).unwrap();
            }
            HostCommand::CallReducer {
                hash,
                reducer_name,
                respond_to,
            } => {
                respond_to.send(self.call_reducer(hash, &reducer_name).await).unwrap();
            }
        }
    }

    fn validate_module(module: &Module) -> Result<(), anyhow::Error> {
        let mut found = false;
        log::trace!("Module validation:");
        for f in module.exports().functions() {
            log::trace!("   {:?}", f);
            if !f.name().starts_with(REDUCE_DUNDER) {
                continue;
            }
            found = true;
            let ty = f.ty();
            if ty.params().len() != 2 {
                return Err(anyhow::anyhow!("Reduce function has wrong number of params."));
            }
            if ty.params()[0] != ValType::I32 {
                return Err(anyhow::anyhow!("Incorrect param type {} for reducer.", ty.params()[0]));
            }
            if ty.params()[1] != ValType::I32 {
                return Err(anyhow::anyhow!("Incorrect param type {} for reducer.", ty.params()[0]));
            }
        }
        if !found {
            return Err(anyhow::anyhow!("Reduce function not found in module."));
        }
        Ok(())
    }

    async fn init_module(&mut self, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let module_hash = self.add_module(wasm_bytes)?;
        let module_host = self.modules.get(&module_hash).unwrap().clone();
        module_host.init_database().await?;
        Ok(module_hash)
    }

    fn add_module(&mut self, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let module_hash = hash_bytes(&wasm_bytes);
        let module = Module::new(&self.store, wasm_bytes)?;

        Self::validate_module(&module)?;

        let identity = hash_bytes(b"");
        let name = "test".into();
        let store = self.store.clone();
        let module_host = ModuleHost::spawn(identity, name, module_hash, module, store);
        self.modules.insert(module_hash, module_host);

        Ok(module_hash)
    }

    async fn call_reducer(&self, hash: Hash, reducer_name: &str) -> Result<(), anyhow::Error> {
        let module_host = self
            .modules
            .get(&hash)
            .ok_or(anyhow::anyhow!("No such module found."))?;
        module_host.call_reducer(reducer_name.into()).await?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct Host {
    tx: mpsc::Sender<HostCommand>,
}

impl Host {
    pub fn spawn() -> Host {
        let (tx, mut rx) = mpsc::channel(8);
        tokio::spawn(async move {
            let mut actor = HostActor::new();
            while let Some(command) = rx.recv().await {
                // TODO: this really shouldn't await, but doing this for now just to get it working
                actor.handle_message(command).await;
            }
        });
        Host { tx }
    }

    pub async fn init_module(&self, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<Hash, anyhow::Error>>();
        self.tx
            .send(HostCommand::InitModule {
                wasm_bytes,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }

    pub async fn add_module(&self, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<Hash, anyhow::Error>>();
        self.tx
            .send(HostCommand::AddModule {
                wasm_bytes,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }

    pub async fn call_reducer(&self, hash: Hash, reducer_name: String) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx
            .send(HostCommand::CallReducer {
                hash,
                reducer_name,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }
}
