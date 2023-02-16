use std::sync::Arc;

use wasmer::wasmparser::Operator;
use wasmer::{
    AsStoreRef, CompilerConfig, EngineBuilder, Memory, MemoryAccessError, Module, RuntimeError, Store, WasmPtr,
};
use wasmer_middlewares::Metering;

use crate::error::NodesError;
use crate::hash::Hash;
use crate::worker_database_instance::WorkerDatabaseInstance;

mod opcode_cost;
mod wasm_instance_env;
mod wasm_module_host_actor;

use wasm_module_host_actor::WasmerModule;

use super::host_controller::Scheduler;
use super::module_host::ModuleHostActor;
use super::wasm_common::DEFAULT_EXECUTION_BUDGET;
use super::wasm_common::{abi, host_actor::WasmModuleHostActor};

pub fn make_actor(
    worker_database_instance: WorkerDatabaseInstance,
    module_hash: Hash,
    program_bytes: Vec<u8>,
    scheduler: Scheduler,
) -> anyhow::Result<Box<impl ModuleHostActor>> {
    let cost_function =
        |operator: &Operator| -> u64 { opcode_cost::OperationType::operation_type_of(operator).energy_cost() };

    // TODO(cloutiertyler): Why are we setting the initial points here? This
    // seems like giving away free energy. Presumably this should always be set
    // before calling reducer?
    // I believe we can just set this to be zero and it's already being set by reducers
    // but I don't want to break things, so I'm going to leave it.
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

    anyhow::ensure!(
        abi == WasmerModule::SUPPORTED_ABI,
        "abi version {abi:?} ({:?}) is not supported",
        abi.as_tuple()
    );

    let module = WasmerModule::new(module, engine);

    WasmModuleHostActor::new(worker_database_instance, module_hash, module, scheduler)
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
enum WasmError {
    Db(#[from] NodesError),
    Mem(#[from] MemoryAccessError),
    Wasm(#[from] RuntimeError),
}

#[derive(Clone)]
struct Mem {
    pub memory: Memory,
}

impl Mem {
    fn extract(exports: &wasmer::Exports) -> anyhow::Result<Self> {
        Ok(Self {
            memory: exports.get_memory("memory")?.clone(),
        })
    }
    fn view(&self, store: &impl AsStoreRef) -> wasmer::MemoryView<'_> {
        self.memory.view(store)
    }
    fn read_bytes(&self, store: &impl AsStoreRef, ptr: WasmPtr<u8>, len: u32) -> Result<Vec<u8>, MemoryAccessError> {
        ptr.slice(&self.view(store), len)?.read_to_vec()
    }
}

#[derive(Copy, Clone, wasmer::ValueType)]
#[repr(transparent)]
struct Buffer {
    raw: u32,
}

impl Buffer {
    fn is_invalid(&self) -> bool {
        self.raw == u32::MAX
    }
}
