use std::borrow::Cow;
use std::time::Duration;

use anyhow::Context;
use spacetimedb_paths::server::{ServerDataDir, WasmtimeCacheDir};
use wasmtime::{self, Engine, Linker, StoreContext, StoreContextMut};

use crate::energy::{EnergyQuanta, ReducerBudget};
use crate::error::NodesError;
use crate::host::module_host::{Instance, ModuleRuntime};
use crate::module_host_context::ModuleCreationContext;

mod wasm_instance_env;
mod wasmtime_module;

use wasmtime_module::{WasmtimeInstance, WasmtimeModule};

use self::wasm_instance_env::WasmInstanceEnv;

use super::wasm_common::module_host_actor::{InitializationError, WasmModuleInstance};
use super::wasm_common::{abi, module_host_actor::WasmModuleHostActor, ModuleCreationError};

pub struct WasmtimeRuntime {
    engine: Engine,
    linker: Box<Linker<WasmInstanceEnv>>,
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
    pub fn new(data_dir: Option<&ServerDataDir>) -> Self {
        let mut config = wasmtime::Config::new();
        config
            .cranelift_opt_level(wasmtime::OptLevel::Speed)
            .consume_fuel(true)
            .epoch_interruption(true)
            .wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable)
            // We need async support to enable suspending execution of procedures
            // when waiting for e.g. HTTP responses or the transaction lock.
            // We don't enable either fuel-based or epoch-based yielding
            // (see https://docs.wasmtime.dev/api/wasmtime/struct.Store.html#method.epoch_deadline_async_yield_and_update
            // and https://docs.wasmtime.dev/api/wasmtime/struct.Store.html#method.fuel_async_yield_interval)
            // so reducers will always execute to completion during the first `Future::poll` call,
            // and procedures will only yield when performing an asynchronous operation.
            // These futures are executed on a separate single-threaded executor not related to the "global" Tokio runtime,
            // which is responsible only for executing WASM. See `crate::util::jobs` for this infrastructure.
            .async_support(true);

        // Offer a compile-time flag for enabling perfmap generation,
        // so `perf` can display JITted symbol names.
        // Ideally we would be able to configure this at runtime via a flag to `spacetime start`,
        // but this is good enough for now.
        #[cfg(feature = "perfmap")]
        config.profiler(wasmtime::ProfilingStrategy::PerfMap);

        // ignore errors for this - if we're not able to set up caching, that's fine, it's just an optimization
        if let Some(data_dir) = data_dir {
            let _ = Self::set_cache_config(&mut config, data_dir.wasmtime_cache());
        }

        let engine = Engine::new(&config).unwrap();

        let weak_engine = engine.weak();
        epoch_ticker(move || {
            let engine = weak_engine.upgrade()?;
            engine.increment_epoch();
            Some(())
        });

        let mut linker = Box::new(Linker::new(&engine));
        WasmtimeModule::link_imports(&mut linker).unwrap();

        WasmtimeRuntime { engine, linker }
    }

    fn set_cache_config(config: &mut wasmtime::Config, cache_dir: WasmtimeCacheDir) -> anyhow::Result<()> {
        use std::io::Write;
        let cache_config = toml::toml! {
            // see <https://docs.wasmtime.dev/cli-cache.html> for options here
            [cache]
            enabled = true
            directory = (toml::Value::try_from(cache_dir.0)?)
        };
        let tmpfile = tempfile::NamedTempFile::new()?;
        write!(&tmpfile, "{cache_config}")?;
        config.cache_config_load(tmpfile.path())?;
        Ok(())
    }
}

pub type Module = WasmModuleHostActor<WasmtimeModule>;
pub type ModuleInstance = WasmModuleInstance<WasmtimeInstance>;

impl ModuleRuntime for WasmtimeRuntime {
    fn make_actor(
        &self,
        mcc: ModuleCreationContext,
    ) -> anyhow::Result<(super::module_host::Module, super::module_host::Instance)> {
        let module =
            wasmtime::Module::new(&self.engine, &mcc.program.bytes).map_err(ModuleCreationError::WasmCompileError)?;

        let func_imports = module
            .imports()
            .filter(|imp| matches!(imp.ty(), wasmtime::ExternType::Func(_)));
        let abi = abi::determine_spacetime_abi(func_imports, |imp| imp.module())?;

        abi::verify_supported(WasmtimeModule::IMPLEMENTED_ABI, abi)?;

        let module = self
            .linker
            .instantiate_pre(&module)
            .map_err(InitializationError::Instantiation)?;

        let module = WasmtimeModule::new(module);

        let (module, init_inst) = WasmModuleHostActor::new(mcc.into_limited(), module)?;
        let module = super::module_host::Module::Wasm(module);
        let init_inst = Instance::Wasm(Box::new(init_inst));

        Ok((module, init_inst))
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

impl WasmtimeFuel {
    /// 1000 energy quanta == 1 wasmtime fuel unit
    const QUANTA_MULTIPLIER: u64 = 1_000;
}

impl From<ReducerBudget> for WasmtimeFuel {
    fn from(v: ReducerBudget) -> Self {
        // ReducerBudget being u64 is load-bearing here - if it was u128 and v was ReducerBudget::MAX,
        // truncating this result would mean that with set_store_fuel(budget.into()), get_store_fuel()
        // would be wildly different than the original `budget`, and the energy usage for the reducer
        // would be u64::MAX even if it did nothing. ask how I know.
        WasmtimeFuel(v.get() / Self::QUANTA_MULTIPLIER)
    }
}

impl From<WasmtimeFuel> for ReducerBudget {
    fn from(v: WasmtimeFuel) -> Self {
        ReducerBudget::new(v.0 * WasmtimeFuel::QUANTA_MULTIPLIER)
    }
}

impl From<WasmtimeFuel> for EnergyQuanta {
    fn from(fuel: WasmtimeFuel) -> Self {
        EnergyQuanta::new(u128::from(fuel.0) * u128::from(WasmtimeFuel::QUANTA_MULTIPLIER))
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
    pub fn view_and_store_mut<'a, T>(&self, store: impl Into<StoreContextMut<'a, T>>) -> (&'a mut MemView, &'a mut T) {
        let (mem, store_data) = self.memory.data_and_store_mut(store);
        (MemView::from_slice_mut(mem), store_data)
    }

    fn view<'a, T: 'a>(&self, store: impl Into<StoreContext<'a, T>>) -> &'a MemView {
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
