use std::borrow::Cow;
use std::sync::Arc;

use anyhow::Context;
use once_cell::sync::Lazy;
use wasmtime::{Engine, Linker, Module, StoreContext, StoreContextMut};

use crate::database_instance_context::DatabaseInstanceContext;
use crate::energy::{EnergyMonitor, EnergyQuanta, ReducerBudget};
use crate::error::NodesError;
use crate::hash::Hash;
use crate::stdb_path;

mod wasm_instance_env;
mod wasmtime_module;

use wasmtime_module::WasmtimeModule;

use self::wasm_instance_env::WasmInstanceEnv;

use super::wasm_common::module_host_actor::InitializationError;
use super::wasm_common::{abi, module_host_actor::WasmModuleHostActor, ModuleCreationError};
use super::Scheduler;

static ENGINE: Lazy<Engine> = Lazy::new(|| {
    let mut config = wasmtime::Config::new();
    config
        .cranelift_opt_level(wasmtime::OptLevel::Speed)
        .consume_fuel(true)
        .wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);

    let cache_config = toml::toml! {
        // see <https://docs.wasmtime.dev/cli-cache.html> for options here
        [cache]
        enabled = true
        directory = (toml::Value::try_from(stdb_path("worker_node/wasmtime_cache")).unwrap())
    };
    // ignore errors for this - if we're not able to set up caching, that's fine, it's just an optimization
    let _ = set_cache_config(&mut config, cache_config);

    Engine::new(&config).unwrap()
});

fn set_cache_config(config: &mut wasmtime::Config, cache_config: toml::value::Table) -> anyhow::Result<()> {
    use std::io::Write;
    let tmpfile = tempfile::NamedTempFile::new()?;
    write!(&tmpfile, "{cache_config}")?;
    config.cache_config_load(tmpfile.path())?;
    Ok(())
}

static LINKER: Lazy<Linker<WasmInstanceEnv>> = Lazy::new(|| {
    let mut linker = Linker::new(&ENGINE);
    WasmtimeModule::link_imports(&mut linker).unwrap();
    linker
});

pub fn make_actor(
    dbic: Arc<DatabaseInstanceContext>,
    module_hash: Hash,
    program_bytes: &[u8],
    scheduler: Scheduler,
    energy_monitor: Arc<dyn EnergyMonitor>,
) -> Result<impl super::module_host::Module, ModuleCreationError> {
    let module = Module::new(&ENGINE, program_bytes).map_err(ModuleCreationError::WasmCompileError)?;

    let func_imports = module
        .imports()
        .filter(|imp| matches!(imp.ty(), wasmtime::ExternType::Func(_)));
    let abi = abi::determine_spacetime_abi(func_imports, |imp| imp.module())?;

    abi::verify_supported(WasmtimeModule::IMPLEMENTED_ABI, abi)?;

    let module = LINKER
        .instantiate_pre(&module)
        .map_err(InitializationError::Instantiation)?;

    let module = WasmtimeModule::new(module);

    WasmModuleHostActor::new(dbic, module_hash, module, scheduler, energy_monitor).map_err(Into::into)
}

#[derive(Debug, derive_more::From)]
enum WasmError {
    Db(NodesError),
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

trait WasmPointee {
    type Pointer;
    fn write_to(self, mem: &mut MemView, ptr: Self::Pointer) -> Result<(), WasmError>;
}
macro_rules! impl_pointee {
    ($($t:ty),*) => {
        $(impl WasmPointee for $t {
            type Pointer = u32;
            fn write_to(self, mem: &mut MemView, ptr: Self::Pointer) -> Result<(), WasmError> {
                let bytes = self.to_le_bytes();
                mem.deref_slice_mut(ptr, bytes.len() as u32)?.copy_from_slice(&bytes);
                Ok(())
            }
        })*
    };
}
impl_pointee!(u8, u16, u32, u64);
impl_pointee!(super::wasm_common::BufferIdx, super::wasm_common::BufferIterIdx);
type WasmPtr<T> = <T as WasmPointee>::Pointer;

/// Wraps access to WASM linear memory with some additional functionality.
#[derive(Clone, Copy)]
struct Mem {
    /// The underlying WASM `memory` instance.
    pub memory: wasmtime::Memory,
}

impl Mem {
    /// Constructs an instance of `Mem` from an exports map.
    fn extract(exports: &wasmtime::Instance, store: impl wasmtime::AsContextMut) -> anyhow::Result<Self> {
        Ok(Self {
            memory: exports.get_memory(store, "memory").context("no memory export")?,
        })
    }

    /// Creates and returns a view into the actual memory `store`.
    /// This view allows for reads and writes.
    fn view_and_store_mut<'a, T>(&self, store: impl Into<StoreContextMut<'a, T>>) -> (&'a mut MemView, &'a mut T) {
        let (mem, store_data) = self.memory.data_and_store_mut(store);
        (MemView::from_slice_mut(mem), store_data)
    }

    fn view<'a, T: 'a>(&self, store: impl Into<StoreContext<'a, T>>) -> &'a MemView {
        MemView::from_slice(self.memory.data(store))
    }
}

#[repr(transparent)]
struct MemView([u8]);

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
    fn deref_slice(&self, offset: WasmPtr<u8>, len: u32) -> Result<&[u8], MemError> {
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
    fn deref_str_lossy(&self, offset: WasmPtr<u8>, len: u32) -> Result<Cow<str>, MemError> {
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
}

/// An error that can result from operations on [`MemView`].
#[derive(thiserror::Error, Debug)]
enum MemError {
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
    fn check_nullptr(self) -> Result<Option<T>, WasmError>;
}
impl<T> NullableMemOp<T> for Result<T, MemError> {
    fn check_nullptr(self) -> Result<Option<T>, WasmError> {
        match self {
            Ok(x) => Ok(Some(x)),
            Err(MemError::Null) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
