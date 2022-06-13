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

lazy_static! {
    pub static ref HOST: Mutex<Host> = Mutex::new(Host::spawn());
}

pub fn get_host() -> Host {
    HOST.lock().unwrap().clone()
}

#[derive(Debug)]
enum HostCommand {
    InitModule {
        identity: Hash,
        name: String,
        wasm_bytes: Vec<u8>,
        respond_to: oneshot::Sender<Result<Hash, anyhow::Error>>,
    },
    UpdateModule {
        identity: Hash,
        name: String,
        wasm_bytes: Vec<u8>,
        respond_to: oneshot::Sender<Result<Hash, anyhow::Error>>,
    },
    AddModule {
        identity: Hash,
        name: String,
        wasm_bytes: Vec<u8>,
        respond_to: oneshot::Sender<Result<Hash, anyhow::Error>>,
    },
    CallReducer {
        identity: Hash,
        name: String,
        reducer_name: String,
        arg_bytes: Vec<u8>,
        respond_to: oneshot::Sender<Result<(), anyhow::Error>>,
    },
}

struct HostActor {
    modules: HashMap<(Hash, String), ModuleHost>,
}

impl HostActor {
    fn new() -> Self {
        let modules: HashMap<(Hash, String), ModuleHost> = HashMap::new();

        Self { modules }
    }

    async fn handle_message(&mut self, message: HostCommand) {
        match message {
            HostCommand::InitModule {
                identity,
                name,
                wasm_bytes,
                respond_to,
            } => {
                respond_to
                    .send(self.init_module(identity, &name, wasm_bytes).await)
                    .unwrap();
            }
            HostCommand::UpdateModule {
                identity,
                name,
                wasm_bytes,
                respond_to,
            } => {
                respond_to
                    .send(self.update_module(identity, &name, wasm_bytes).await)
                    .unwrap();
            }
            HostCommand::AddModule {
                identity,
                name,
                wasm_bytes,
                respond_to,
            } => {
                respond_to.send(self.add_module(identity, &name, wasm_bytes).await).unwrap();
            }
            HostCommand::CallReducer {
                identity,
                name,
                reducer_name,
                arg_bytes,
                respond_to,
            } => {
                respond_to
                    .send(self.call_reducer(identity, &name, &reducer_name, arg_bytes).await)
                    .unwrap();
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

    async fn init_module(&mut self, identity: Hash, name: &str, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let module_hash = self.add_module(identity, name, wasm_bytes).await?;
        let key = (identity, name.to_string());
        let module_host = self.modules.get(&key).unwrap().clone();
        module_host.init_database().await?;
        Ok(module_hash)
    }
    
    async fn update_module(&mut self, identity: Hash, name: &str, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let module_hash = self.add_module(identity, name, wasm_bytes).await?;
        let key = (identity, name.to_string());
        let module_host = self.modules.get(&key).unwrap().clone();
        module_host.migrate_database().await?;
        Ok(module_hash)
    }

    async fn add_module(&mut self, identity: Hash, name: &str, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let module_hash = hash_bytes(&wasm_bytes);
        let key = (identity, name.to_string());
        if let Some(module_host) = self.modules.get(&key) {
            module_host.exit().await?;
        }

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
        let module = Module::new(&store, wasm_bytes)?;

        Self::validate_module(&module)?;

        let module_host = ModuleHost::spawn(identity, name.into(), module_hash, module, store);
        self.modules.insert(key, module_host);

        Ok(module_hash)
    }

    async fn call_reducer(
        &self,
        identity: Hash,
        name: &str,
        reducer_name: &str,
        arg_bytes: impl AsRef<[u8]>,
    ) -> Result<(), anyhow::Error> {
        let key = (identity, name.to_string());
        let module_host = self
            .modules
            .get(&key)
            .ok_or(anyhow::anyhow!("No such module found."))?;
        module_host
            .call_reducer(reducer_name.into(), arg_bytes.as_ref().to_vec())
            .await?;
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

    pub async fn init_module(&self, identity: Hash, name: String, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<Hash, anyhow::Error>>();
        self.tx
            .send(HostCommand::InitModule {
                identity,
                name,
                wasm_bytes,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }
    
    pub async fn update_module(&self, identity: Hash, name: String, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<Hash, anyhow::Error>>();
        self.tx
            .send(HostCommand::UpdateModule {
                identity,
                name,
                wasm_bytes,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }

    pub async fn add_module(&self, identity: Hash, name: String, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<Hash, anyhow::Error>>();
        self.tx
            .send(HostCommand::AddModule {
                identity,
                name,
                wasm_bytes,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }

    pub async fn call_reducer(
        &self,
        identity: Hash,
        name: String, 
        reducer_name: String,
        arg_bytes: Vec<u8>,
    ) -> Result<(), anyhow::Error> {
        let (tx, rx) = oneshot::channel::<Result<(), anyhow::Error>>();
        self.tx
            .send(HostCommand::CallReducer {
                identity,
                name,
                reducer_name,
                arg_bytes,
                respond_to: tx,
            })
            .await?;
        rx.await.unwrap()
    }
}
