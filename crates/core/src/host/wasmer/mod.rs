use std::sync::Arc;

use wasmer::wasmparser::Operator;
use wasmer::{AsStoreRef, CompilerConfig, EngineBuilder, Memory, MemoryAccessError, Module, RuntimeError, WasmPtr};
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

    let engine: wasmer::Engine = EngineBuilder::new(compiler_config).into();

    let module = Module::new(&engine, program_bytes).map_err(|e| ModuleCreationError::WasmCompileError(e.into()))?;

    let abi = abi::determine_spacetime_abi(module.imports().functions(), wasmer::ImportType::module)?;

    if let Some(abi) = abi {
        abi::verify_supported(WasmerModule::IMPLEMENTED_ABI, abi)?;
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

/// Wraps access to WASM linear memory with some additional functionality.
#[derive(Clone)]
struct Mem {
    /// The underlying WASM `memory` instance.
    pub memory: Memory,
}

impl Mem {
    /// Constructs an instance of `Mem` from an exports map.
    fn extract(exports: &wasmer::Exports) -> anyhow::Result<Self> {
        let memory = exports.get_memory("memory")?.clone();
        Ok(Self { memory })
    }

    /// Creates and returns a view into the actual memory `store`.
    /// This view allows for reads and writes.

    fn view<'a>(&self, store: &'a impl AsStoreRef) -> wasmer::MemoryView<'a> {
        self.memory.view(store)
    }

    /// Reads a slice of bytes starting from `ptr`
    /// and lasting `len` bytes into a `Vec<u8>`.
    ///
    /// Returns an error if the slice length overflows a 64-bit address.
    fn read_bytes(&self, store: &impl AsStoreRef, ptr: WasmPtr<u8>, len: u32) -> Result<Vec<u8>, MemoryAccessError> {
        ptr.slice(&self.view(store), len)?.read_to_vec()
    }

    /// Writes `data` into the slice starting from `ptr`
    /// and lasting `len` bytes.
    ///
    /// Returns an error if
    /// - the slice length overflows a 64-bit address
    /// - `len != data.len()`
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
