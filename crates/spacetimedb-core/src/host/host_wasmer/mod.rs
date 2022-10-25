use std::sync::Arc;

use anyhow::Context;
use wasmer::wasmparser::Operator;
use wasmer::{
    AsStoreMut, AsStoreRef, CompilerConfig, EngineBuilder, Memory, MemoryAccessError, Module, Store, TypedFunction,
    WasmPtr,
};
use wasmer_middlewares::Metering;

use crate::hash::Hash;
use crate::host::module_host::ModuleHost;
use crate::worker_database_instance::WorkerDatabaseInstance;

mod opcode_cost;
mod wasm_instance_env;
mod wasm_module_host_actor;

use wasm_module_host_actor::{WasmerModule, DEFAULT_EXECUTION_BUDGET};

use super::wasm_common::{abi, host_actor::WasmModuleHostActor};

pub fn make_wasmer_module_host_actor(
    worker_database_instance: WorkerDatabaseInstance,
    module_hash: Hash,
    program_bytes: Vec<u8>,
    trace_log: bool,
) -> Result<ModuleHost, anyhow::Error> {
    ModuleHost::spawn(worker_database_instance.identity, |_| {
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

        let engine = EngineBuilder::new(compiler_config).engine();

        let store = Store::new(&engine);
        let module = Module::new(&store, &program_bytes)?;

        let abi = abi::determine_spacetime_abi(&program_bytes)?;

        let module = WasmerModule::new(module, engine, abi);

        Ok(Box::new(WasmModuleHostActor::new(
            worker_database_instance,
            module_hash,
            trace_log,
            module,
        )?))
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
