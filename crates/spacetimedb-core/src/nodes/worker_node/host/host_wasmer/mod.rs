use std::sync::Arc;

use anyhow::Context;
use wasmer::wasmparser::Operator;
use wasmer::{
    AsStoreMut, AsStoreRef, CompilerConfig, EngineBuilder, Memory, MemoryAccessError, Module, Store, Type,
    TypedFunction, WasmPtr,
};
use wasmer_middlewares::Metering;

use crate::hash::Hash;
use crate::nodes::worker_node::host::module_host::ModuleHost;
use crate::nodes::worker_node::worker_database_instance::WorkerDatabaseInstance;

mod opcode_cost;
mod wasm_instance_env;
pub mod wasm_module_host_actor;

use wasm_module_host_actor::{WasmModuleHostActor, DEFAULT_EXECUTION_BUDGET};

use super::wasm_common::abi;

const REDUCE_DUNDER: &str = "__reducer__";
const REPEATING_REDUCER_DUNDER: &str = "__repeating_reducer__";

fn validate_module(module: &Module) -> Result<(), anyhow::Error> {
    let mut found = false;
    for f in module.exports().functions() {
        let ty = f.ty();
        log::trace!("   {:?}", f);
        if f.name().starts_with(REDUCE_DUNDER) {
            if ty.params().len() != 2 {
                return Err(anyhow::anyhow!("Reduce function has wrong number of params."));
            }
            if ty.params()[0] != Type::I32 {
                return Err(anyhow::anyhow!("Incorrect param type {} for reducer.", ty.params()[0]));
            }
            if ty.params()[1] != Type::I32 {
                return Err(anyhow::anyhow!("Incorrect param type {} for reducer.", ty.params()[0]));
            }
            found = true;
        } else if f.name().starts_with(REPEATING_REDUCER_DUNDER) {
            if ty.params().len() != 2 {
                return Err(anyhow::anyhow!("Reduce function has wrong number of params."));
            }
            if ty.params()[0] != Type::I32 {
                return Err(anyhow::anyhow!(
                    "Incorrect param type {} for repeating reducer.",
                    ty.params()[0]
                ));
            }
            if ty.params()[1] != Type::I32 {
                return Err(anyhow::anyhow!(
                    "Incorrect param type {} for repeating reducer.",
                    ty.params()[0]
                ));
            }
            found = true;
        }
    }
    if !found {
        return Err(anyhow::anyhow!("Reduce function not found in module."));
    }
    Ok(())
}

pub fn make_wasmer_module_host_actor(
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

        let store = Store::new(&EngineBuilder::new(compiler_config).engine());
        let module = Module::new(&store, &program_bytes)?;

        let abi = abi::determine_spacetime_abi(&program_bytes)?;

        let address = worker_database_instance.address;
        log::trace!("Validating module for database: \"{}\"", address.to_hex());
        validate_module(&module)?;

        let host = WasmModuleHostActor::new(worker_database_instance, module_hash, module, store, module_host, abi)?;
        Ok(Box::from(host))
    })
}

#[derive(Clone)]
pub struct Mem {
    pub memory: Memory,
    pub alloc: TypedFunction<u32, WasmPtr<u8>>,
    pub dealloc: TypedFunction<(WasmPtr<u8>, u32), ()>,
}

impl Mem {
    pub fn extract(store: &impl AsStoreRef, exports: &wasmer::Exports) -> anyhow::Result<Self> {
        Ok(Self {
            memory: exports.get_memory("memory")?.clone(),
            alloc: exports.get_typed_function(store, "alloc")?,
            dealloc: exports.get_typed_function(store, "dealloc")?,
        })
    }
    pub fn read_output_bytes(
        &self,
        store: &impl AsStoreRef,
        ptr: WasmPtr<u8>,
        len: u32,
    ) -> Result<Vec<u8>, MemoryAccessError> {
        ptr.slice(&self.memory.view(store), len)?.read_to_vec()
    }
    pub fn dealloc(&self, store: &mut impl AsStoreMut, ptr: WasmPtr<u8>, len: u32) -> Result<(), wasmer::RuntimeError> {
        self.dealloc.call(store, ptr, len)
    }
    pub fn alloc_slice(&self, store: &mut impl AsStoreMut, data: &[u8]) -> anyhow::Result<(u32, u32)> {
        let data_len = data.len().try_into().context("data too big to alloc to wasm")?;
        let ptr = self.alloc.call(store, data_len).context("alloc failed")?;

        ptr.slice(&self.memory.view(store), data_len)
            .context("alloc out of bounds")?
            .write_slice(data)?;

        Ok((ptr.offset(), data_len))
    }
}
