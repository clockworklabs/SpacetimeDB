use std::sync::Arc;

use wasmer::wasmparser::Operator;
use wasmer::{
    AsStoreRef, CompilerConfig, EngineBuilder, Memory, MemoryAccessError, Module, RuntimeError, Store, WasmPtr,
};
use wasmer_middlewares::Metering;

use crate::database_instance_context::DatabaseInstanceContext;
use crate::error::NodesError;
use crate::hash::Hash;

mod opcode_cost;
mod wasm_instance_env;
mod wasmer_module;

use wasmer_module::WasmerModule;

use super::scheduler::Scheduler;
use super::wasm_common::{abi, module_host_actor::WasmModuleHostActor, ModuleCreationError};
use super::{EnergyMonitor, EnergyQuanta};

pub fn make_actor(
    dbic: Arc<DatabaseInstanceContext>,
    module_hash: Hash,
    program_bytes: &[u8],
    scheduler: Scheduler,
    energy_monitor: Arc<dyn EnergyMonitor>,
) -> Result<impl super::module_host::Module, ModuleCreationError> {
    let cost_function =
        |operator: &Operator| -> u64 { opcode_cost::OperationType::operation_type_of(operator).energy_cost() };

    // TODO(cloutiertyler): Why are we setting the initial points here? This
    // seems like giving away free energy. Presumably this should always be set
    // before calling reducer?
    // I believe we can just set this to be zero and it's already being set by reducers
    // but I don't want to break things, so I'm going to leave it.
    let initial_points = EnergyQuanta::DEFAULT_BUDGET.as_points();
    let metering = Arc::new(Metering::new(initial_points, cost_function));

    // let mut compiler_config = wasmer_compiler_llvm::LLVM::default();
    // compiler_config.opt_level(wasmer_compiler_llvm::LLVMOptLevel::Aggressive);
    // compiler_config.push_middleware(metering);
    let mut compiler_config = wasmer::Cranelift::default();
    compiler_config.opt_level(wasmer::CraneliftOptLevel::Speed);
    compiler_config.push_middleware(metering);

    let engine = EngineBuilder::new(compiler_config).engine();

    let store = Store::new(&engine);
    let module = Module::new(&store, program_bytes).map_err(|e| ModuleCreationError::WasmCompileError(e.into()))?;

    let abi = abi::determine_spacetime_abi(program_bytes)?;

    if !WasmerModule::IMPLEMENTED_ABI.supports(abi) {
        return Err(ModuleCreationError::Abi(abi::AbiVersionError::UnsupportedVersion {
            implement: WasmerModule::IMPLEMENTED_ABI,
            got: abi,
        }));
    }

    let module = WasmerModule::new(module, engine);

    WasmModuleHostActor::new(dbic, module_hash, module, scheduler, energy_monitor).map_err(Into::into)
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

    /// Reads a slice of bytes starting from `ptr` and lasting `len` bytes into a `Vec<u8>`.
    ///
    /// Returns an error if the slice length overflows a 64-bit address.
    fn read_bytes(&self, store: &impl AsStoreRef, ptr: WasmPtr<u8>, len: u32) -> Result<Vec<u8>, MemoryAccessError> {
        ptr.slice(&self.view(store), len)?.read_to_vec()
    }
    fn set_bytes(
        &self,
        store: &impl AsStoreRef,
        ptr: WasmPtr<u8>,
        len: u32,
        data: &[u8],
    ) -> Result<(), MemoryAccessError> {
        ptr.slice(&self.view(store), len)?.write_slice(data)
    }
}
