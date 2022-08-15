
use crate::hash::{hash_bytes, Hash, ToHexString};
use anyhow;
use lazy_static::lazy_static;
use log;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use wasmer::{wasmparser::Operator, CompilerConfig, Module, Store, Universal, ValType};
use wasmer_middlewares::Metering;
use super::{wasm_module_host::ModuleHost, worker_database_instance::WorkerDatabaseInstance};

const REDUCE_DUNDER: &str = "__reducer__";

lazy_static! {
    pub static ref HOST: WasmHostController = WasmHostController::new();
}

pub fn get_host() -> &'static WasmHostController {
    &HOST
}

pub struct WasmHostController {
    modules: Mutex<HashMap<u64, ModuleHost>>,
}

impl WasmHostController {
    fn new() -> Self {
        let modules = Mutex::new(HashMap::new());
        Self { modules }
    }

    fn validate_module(module: &Module) -> Result<(), anyhow::Error> {
        let mut found = false;
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

    pub async fn init_module(&self, worker_database_instance: WorkerDatabaseInstance, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let key = worker_database_instance.database_instance_id;
        let module_hash = self.spawn_module(worker_database_instance, wasm_bytes).await?;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules.get(&key).unwrap().clone()
        };
        module_host.init_database().await?;
        module_host.start_repeating_reducers().await?;
        Ok(module_hash)
    }

    pub async fn delete_module(&self, worker_database_instance_id: u64) -> Result<(), anyhow::Error> {
        let key = worker_database_instance_id;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules.get(&key).map(|x| x.clone())
        };
        if let Some(module_host) = module_host {
            module_host.delete_database().await?;
        }
        let mut modules = self.modules.lock().unwrap();
        modules.remove(&key);
        Ok(())
    }

    pub async fn update_module(&self, worker_database_instance: WorkerDatabaseInstance, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let key = worker_database_instance.database_instance_id;
        let module_hash = self.spawn_module(worker_database_instance, wasm_bytes).await?;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules.get(&key).unwrap().clone()
        };
        module_host.migrate_database().await?;
        module_host.start_repeating_reducers().await?;
        Ok(module_hash)
    }

    pub async fn add_module(&self, worker_database_instance: WorkerDatabaseInstance, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let key = worker_database_instance.database_instance_id;
        let module_hash = self.spawn_module(worker_database_instance, wasm_bytes).await?;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules.get(&key).unwrap().clone()
        };
        module_host.start_repeating_reducers().await?;
        Ok(module_hash)
    }

    pub async fn spawn_module(&self, worker_database_instance: WorkerDatabaseInstance, wasm_bytes: Vec<u8>) -> Result<Hash, anyhow::Error> {
        let module_hash = hash_bytes(&wasm_bytes);
        let key = worker_database_instance.database_instance_id;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules.get(&key).map(|x| x.clone())
        };
        if let Some(module_host) = module_host {
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

        // let mut compiler_config = wasmer_compiler_llvm::LLVM::default();
        // compiler_config.opt_level(wasmer_compiler_llvm::LLVMOptLevel::Aggressive);
        // compiler_config.push_middleware(metering);
        let mut compiler_config = wasmer::Cranelift::default();
        compiler_config.opt_level(wasmer::CraneliftOptLevel::Speed);
        compiler_config.push_middleware(metering);

        let store = Store::new(&Universal::new(compiler_config).engine());
        let module = Module::new(&store, wasm_bytes)?;

        let identity = worker_database_instance.identity;
        let name = &worker_database_instance.name;
        log::trace!("Validating module \"{}/{}\":", identity.to_hex_string(), name);
        Self::validate_module(&module)?;

        let module_host = ModuleHost::spawn(worker_database_instance, module_hash, module, store);
        let mut modules = self.modules.lock().unwrap();
        modules.insert(key, module_host);

        Ok(module_hash)
    }

    pub async fn call_reducer(
        &self,
        instance_id: u64,
        caller_identity: Hash,
        reducer_name: &str,
        arg_bytes: impl AsRef<[u8]>,
    ) -> Result<(), anyhow::Error> {
        let key = instance_id;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules.get(&key).ok_or(anyhow::anyhow!("No such module found."))?.clone()
        };
        module_host
            .call_reducer(caller_identity, reducer_name.into(), arg_bytes.as_ref().to_vec())
            .await?;
        Ok(())
    }

    pub fn get_module(&self, instance_id: u64) -> Result<ModuleHost, anyhow::Error> {
        let key = instance_id;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules.get(&key).ok_or(anyhow::anyhow!("No such module found."))?.clone()
        };
        Ok(module_host)
    }
}