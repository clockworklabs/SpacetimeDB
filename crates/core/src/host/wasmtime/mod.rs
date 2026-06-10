use self::wasm_instance_env::WasmInstanceEnv;
use super::wasm_common::module_host_actor::{InitializationError, WasmModuleHostActor, WasmModuleInstance};
use super::wasm_common::{abi, ModuleCreationError};
use crate::config::WasmConfig;
use crate::energy::FunctionBudget;
use crate::error::NodesError;
use crate::module_host_context::ModuleCreationContext;
use crate::util::jobs::AllocatedJobCore;
use anyhow::Context;
use spacetimedb_paths::server::ServerDataDir;
use std::borrow::Cow;
use std::time::Duration;
pub(in crate::host) use wasm_instance_env::WasmMemoryBytesMetric;
use wasmtime::{self, Engine, Linker, StoreContext, StoreContextMut};
pub use wasmtime_module::{WasmtimeAsyncModule, WasmtimeInstance, WasmtimeModule};

#[cfg(unix)]
mod pooling_stack_creator;
mod wasm_instance_env;
mod wasmtime_module;

pub struct WasmtimeRuntime {
    sync_engine: Engine,
    sync_linker: Box<Linker<WasmInstanceEnv>>,
    async_engine: Engine,
    async_linker: Box<Linker<WasmInstanceEnv>>,
    config: WasmConfig,
}

const EPOCH_TICK_LENGTH: Duration = Duration::from_millis(10);

pub(crate) const EPOCH_TICKS_PER_SECOND: u64 = ticks_in_duration(Duration::from_secs(1));

pub(crate) const fn ticks_in_duration(duration: Duration) -> u64 {
    duration.div_duration_f64(EPOCH_TICK_LENGTH) as u64
}

pub(crate) fn epoch_ticker(mut on_tick: impl 'static + Send + FnMut() -> Option<()>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(EPOCH_TICK_LENGTH);
        loop {
            interval.tick().await;
            let Some(()) = on_tick() else {
                return;
            };
        }
    });
}

impl WasmtimeRuntime {
    pub fn new(data_dir: Option<&ServerDataDir>, runtime_config: WasmConfig) -> Self {
        let sync_config = wasmtime_config(data_dir, false);
        let async_config = wasmtime_config(data_dir, true);

        let sync_engine = Engine::new(&sync_config).unwrap();
        let async_engine = Engine::new(&async_config).unwrap();

        let weak_sync_engine = sync_engine.weak();
        let weak_async_engine = async_engine.weak();
        epoch_ticker(move || {
            let mut ticked = false;
            if let Some(engine) = weak_sync_engine.upgrade() {
                engine.increment_epoch();
                ticked = true;
            }
            if let Some(engine) = weak_async_engine.upgrade() {
                engine.increment_epoch();
                ticked = true;
            }
            ticked.then_some(())
        });

        let mut sync_linker = Box::new(Linker::new(&sync_engine));
        WasmtimeModule::link_imports(&mut sync_linker).unwrap();

        let mut async_linker = Box::new(Linker::new(&async_engine));
        WasmtimeAsyncModule::link_imports(&mut async_linker).unwrap();

        let config = runtime_config;
        WasmtimeRuntime {
            sync_engine,
            sync_linker,
            async_engine,
            async_linker,
            config,
        }
    }
}

fn wasmtime_config(data_dir: Option<&ServerDataDir>, async_support: bool) -> wasmtime::Config {
    let mut config = wasmtime::Config::new();
    config
        .cranelift_opt_level(wasmtime::OptLevel::Speed)
        .consume_fuel(true)
        .epoch_interruption(true)
        .wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);

    if async_support {
        // Procedure instances need async support to suspend execution when waiting for
        // e.g. HTTP responses or the transaction lock. Main-lane instances use a
        // separate sync engine so reducers/views do not pay Wasmtime's fiber overhead.
        config.async_support(true);

        #[cfg(unix)]
        config
            .async_stack_size(self::pooling_stack_creator::ASYNC_STACK_SIZE)
            .with_host_stack(self::pooling_stack_creator::PoolingStackCreator::new());
    }

    // Offer a compile-time flag for enabling perfmap generation,
    // so `perf` can display JITted symbol names.
    // Ideally we would be able to configure this at runtime via a flag to `spacetime start`,
    // but this is good enough for now.
    #[cfg(feature = "perfmap")]
    config.profiler(wasmtime::ProfilingStrategy::PerfMap);

    if let Some(data_dir) = data_dir {
        let mut cache_config = wasmtime::CacheConfig::new();
        cache_config.with_directory(data_dir.wasmtime_cache().0);
        match wasmtime::Cache::new(cache_config) {
            Ok(cache) => {
                config.cache(Some(cache));
            }
            Err(e) => {
                // caching is just an optimization, so if it fails, just log and continue
                tracing::warn!("failed to set up wasmtime cache: {e:#}")
            }
        }
    }

    config
}

pub type Module = WasmModuleHostActor<WasmtimeModule>;
pub type ProcedureModule = WasmModuleHostActor<WasmtimeAsyncModule>;
pub type ModuleInstance = WasmModuleInstance<WasmtimeInstance>;

// Linux thread names expose at most 15 bytes, so keep the database identity
// suffix short enough to survive after the `wasm-` prefix.
const THREAD_NAME_DATABASE_ID_SUFFIX_LEN: usize = 10;

fn wasm_worker_thread_name(database_identity: &spacetimedb_lib::Identity) -> String {
    let hex = database_identity.to_hex();
    // We use the tail of the identity to avoid the common structured prefix.
    let suffix = &hex.as_str()[hex.as_str().len() - THREAD_NAME_DATABASE_ID_SUFFIX_LEN..];
    format!("wasm-{suffix}")
}

impl WasmtimeRuntime {
    pub fn make_actor(
        &self,
        mcc: ModuleCreationContext,
        program_bytes: &[u8],
        core: AllocatedJobCore,
    ) -> anyhow::Result<super::module_host::ModuleWithInstance> {
        let module =
            wasmtime::Module::new(&self.sync_engine, program_bytes).map_err(ModuleCreationError::WasmCompileError)?;

        let func_imports = module
            .imports()
            .filter(|imp| matches!(imp.ty(), wasmtime::ExternType::Func(_)));
        let abi = abi::determine_spacetime_abi(func_imports, |imp| imp.module())?;

        abi::verify_supported(WasmtimeModule::IMPLEMENTED_ABI, abi)?;

        let module = self
            .sync_linker
            .instantiate_pre(&module)
            .map_err(InitializationError::Instantiation)?;
        let procedure_module =
            wasmtime::Module::new(&self.async_engine, program_bytes).map_err(ModuleCreationError::WasmCompileError)?;
        let procedure_module = self
            .async_linker
            .instantiate_pre(&procedure_module)
            .map_err(InitializationError::Instantiation)?;

        let module = WasmtimeModule::new(module);
        let procedure_module = WasmtimeAsyncModule::new(procedure_module);
        let thread_name = wasm_worker_thread_name(&mcc.replica_ctx.database_identity);

        let (module, init_inst) = WasmModuleHostActor::new(mcc, module)?;
        let procedure_module = module.with_runtime_module(procedure_module)?;
        Ok(super::module_host::ModuleWithInstance::Wasm {
            module,
            procedure_module,
            thread_name,
            core,
            init_inst: Box::new(init_inst),
            procedure_instance_pool_size: self.config.procedure_instance_pool_size,
        })
    }
}

#[derive(Debug, derive_more::From)]
pub enum WasmError {
    Db(NodesError),
    BufferTooSmall,
    Wasm(anyhow::Error),
}

#[derive(Copy, Clone)]
struct WasmtimeFuel(u64);

impl WasmtimeFuel {}

impl From<FunctionBudget> for WasmtimeFuel {
    fn from(v: FunctionBudget) -> Self {
        // FunctionBudget being u64 is load-bearing here - if it was u128 and v was FunctionBudget::MAX,
        // truncating this result would mean that with set_store_fuel(budget.into()), get_store_fuel()
        // would be wildly different than the original `budget`, and the energy usage for the reducer
        // would be u64::MAX even if it did nothing. ask how I know.
        WasmtimeFuel(v.get())
    }
}

impl From<WasmtimeFuel> for FunctionBudget {
    fn from(v: WasmtimeFuel) -> Self {
        FunctionBudget::new(v.0)
    }
}

pub trait WasmPointee {
    type Pointer;
    fn write_to(self, mem: &mut MemView, ptr: Self::Pointer) -> Result<(), MemError>;
    fn read_from(mem: &mut MemView, ptr: Self::Pointer) -> Result<Self, MemError>
    where
        Self: Sized;
}
macro_rules! impl_pointee {
    ($($t:ty),*) => {
        $(impl WasmPointee for $t {
            type Pointer = u32;
            fn write_to(self, mem: &mut MemView, ptr: Self::Pointer) -> Result<(), MemError> {
                let bytes = self.to_le_bytes();
                mem.deref_slice_mut(ptr, bytes.len() as u32)?.copy_from_slice(&bytes);
                Ok(())
            }
            fn read_from(mem: &mut MemView, ptr: Self::Pointer) -> Result<Self, MemError> {
                Ok(Self::from_le_bytes(*mem.deref_array(ptr)?))
            }
        })*
    };
}
impl_pointee!(u8, u16, u32, u64);
impl_pointee!(super::wasm_common::RowIterIdx);

impl WasmPointee for spacetimedb_lib::Identity {
    type Pointer = u32;
    fn write_to(self, mem: &mut MemView, ptr: Self::Pointer) -> Result<(), MemError> {
        let bytes = self.to_byte_array();
        mem.deref_slice_mut(ptr, bytes.len() as u32)?.copy_from_slice(&bytes);
        Ok(())
    }
    fn read_from(mem: &mut MemView, ptr: Self::Pointer) -> Result<Self, MemError> {
        Ok(Self::from_byte_array(*mem.deref_array(ptr)?))
    }
}

impl WasmPointee for spacetimedb_lib::ConnectionId {
    type Pointer = u32;
    fn write_to(self, mem: &mut MemView, ptr: Self::Pointer) -> Result<(), MemError> {
        let bytes = self.as_le_byte_array();
        mem.deref_slice_mut(ptr, bytes.len() as u32)?.copy_from_slice(&bytes);
        Ok(())
    }
    fn read_from(mem: &mut MemView, ptr: Self::Pointer) -> Result<Self, MemError> {
        Ok(Self::from_le_byte_array(*mem.deref_array(ptr)?))
    }
}

type WasmPtr<T> = <T as WasmPointee>::Pointer;

/// Wraps access to WASM linear memory with some additional functionality.
#[derive(Clone, Copy)]
pub struct Mem {
    /// The underlying WASM `memory` instance.
    pub memory: wasmtime::Memory,
}

impl Mem {
    /// Constructs an instance of `Mem` from an exports map.
    pub fn extract(exports: &wasmtime::Instance, store: impl wasmtime::AsContextMut) -> anyhow::Result<Self> {
        Ok(Self {
            memory: exports.get_memory(store, "memory").context("no memory export")?,
        })
    }

    /// Creates and returns a view into the actual memory `store`.
    /// This view allows for reads and writes.
    pub fn view_and_store_mut<'a, T: 'static>(
        &self,
        store: impl Into<StoreContextMut<'a, T>>,
    ) -> (&'a mut MemView, &'a mut T) {
        let (mem, store_data) = self.memory.data_and_store_mut(store);
        (MemView::from_slice_mut(mem), store_data)
    }

    fn view<'a, T: 'static>(&self, store: impl Into<StoreContext<'a, T>>) -> &'a MemView {
        MemView::from_slice(self.memory.data(store))
    }
}

#[repr(transparent)]
pub struct MemView([u8]);

impl MemView {
    fn from_slice_mut(v: &mut [u8]) -> &mut Self {
        // SAFETY: MemView is repr(transparent) over [u8]
        unsafe { &mut *(v as *mut [u8] as *mut MemView) }
    }
    fn from_slice(v: &[u8]) -> &Self {
        // SAFETY: MemView is repr(transparent) over [u8]
        unsafe { &*(v as *const [u8] as *const MemView) }
    }

    /// Get a byte slice of wasm memory given a pointer and a length.
    pub fn deref_slice(&self, offset: WasmPtr<u8>, len: u32) -> Result<&[u8], MemError> {
        if offset == 0 {
            return Err(MemError::Null);
        }
        self.0
            .get(offset as usize..)
            .and_then(|s| s.get(..len as usize))
            .ok_or(MemError::OutOfBounds)
    }

    /// Get a utf8 slice of wasm memory given a pointer and a length.
    fn deref_str(&self, offset: WasmPtr<u8>, len: u32) -> Result<&str, MemError> {
        let b = self.deref_slice(offset, len)?;
        std::str::from_utf8(b).map_err(MemError::Utf8)
    }

    /// Lossily get a utf8 slice of wasm memory given a pointer and a length, converting any
    /// non-utf8 bytes to `U+FFFD REPLACEMENT CHARACTER`.
    fn deref_str_lossy(&self, offset: WasmPtr<u8>, len: u32) -> Result<Cow<'_, str>, MemError> {
        self.deref_slice(offset, len).map(String::from_utf8_lossy)
    }

    /// Get a mutable byte slice of wasm memory given a pointer and a length;
    fn deref_slice_mut(&mut self, offset: WasmPtr<u8>, len: u32) -> Result<&mut [u8], MemError> {
        if offset == 0 {
            return Err(MemError::Null);
        }
        self.0
            .get_mut(offset as usize..)
            .and_then(|s| s.get_mut(..len as usize))
            .ok_or(MemError::OutOfBounds)
    }

    /// Get a byte array of wasm memory the size of `N`.
    fn deref_array<const N: usize>(&self, offset: WasmPtr<u8>) -> Result<&[u8; N], MemError> {
        Ok(self.deref_slice(offset, N as u32)?.try_into().unwrap())
    }
}

/// An error that can result from operations on [`MemView`].
#[derive(thiserror::Error, Debug)]
pub enum MemError {
    #[error("out of bounds pointer passed to a spacetime function")]
    OutOfBounds,
    #[error("null pointer passed to a spacetime function")]
    Null,
    #[error("invalid utf8 passed to a spacetime function")]
    Utf8(#[from] std::str::Utf8Error),
}

impl From<MemError> for WasmError {
    fn from(err: MemError) -> Self {
        WasmError::Wasm(err.into())
    }
}

/// Extension trait to gracefully handle null `WasmPtr`s, e.g.
/// `mem.deref_slice(ptr, len).check_nullptr()? == Option<&[u8]>`.
trait NullableMemOp<T> {
    fn check_nullptr(self) -> Result<Option<T>, MemError>;
}
impl<T> NullableMemOp<T> for Result<T, MemError> {
    fn check_nullptr(self) -> Result<Option<T>, MemError> {
        match self {
            Ok(x) => Ok(Some(x)),
            Err(MemError::Null) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
