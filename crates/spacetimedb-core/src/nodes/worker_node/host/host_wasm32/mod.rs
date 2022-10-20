use std::sync::Arc;

use wasmer::wasmparser::Operator;
use wasmer::{CompilerConfig, Module, Store, Universal, ValType};
use wasmer_middlewares::Metering;

use crate::hash::Hash;
use crate::nodes::worker_node::host::host_wasm32::wasm_module_host_actor::{
    WasmModuleHostActor, DEFAULT_EXECUTION_BUDGET,
};
use crate::nodes::worker_node::host::module_host::ModuleHost;
use crate::nodes::worker_node::worker_database_instance::WorkerDatabaseInstance;

mod opcode_cost;
mod wasm_instance_env;
pub mod wasm_module_host_actor;

const REDUCE_DUNDER: &str = "__reducer__";

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

pub fn make_wasm32_module_host_actor(
    worker_database_instance: WorkerDatabaseInstance,
    module_hash: Hash,
    program_bytes: Vec<u8>,
) -> Result<ModuleHost, anyhow::Error> {
    ModuleHost::spawn(worker_database_instance.identity, |module_host| {
        let cost_function =
            |operator: &Operator| -> u64 { opcode_cost::OperationType::operation_type_of(operator).energy_cost() };
        let initial_points = DEFAULT_EXECUTION_BUDGET as u64;
        let metering = Arc::new(Metering::new(initial_points, cost_function));

        // let mut compiler_config = wasmer_compiler_llvm::LLVM::default();
        // compiler_config.opt_level(wasmer_compiler_llvm::LLVMOptLevel::Aggressive);
        // compiler_config.push_middleware(metering);
        let mut compiler_config = wasmer::Cranelift::default();
        compiler_config.opt_level(wasmer::CraneliftOptLevel::Speed);
        compiler_config.push_middleware(metering);

        let store = Store::new(&Universal::new(compiler_config).engine());
        let module = Module::new(&store, program_bytes)?;

        let address = worker_database_instance.address;
        log::trace!("Validating module for database: \"{}\"", address.to_hex());
        validate_module(&module)?;

        let host = WasmModuleHostActor::new(worker_database_instance, module_hash, module, store, module_host)?;
        Ok(Box::from(host))
    })
}
