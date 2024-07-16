pub mod abi;
pub mod instrumentation;
pub mod module_host_actor;

use std::num::NonZeroU16;
use std::time::Instant;

use super::AbiCall;
use crate::error::{DBError, IndexError, NodesError};
use spacetimedb_primitives::errno;
use spacetimedb_sats::typespace::TypeRefError;
use spacetimedb_table::table::UniqueConstraintViolation;

pub const CALL_REDUCER_DUNDER: &str = "__call_reducer__";

pub const DESCRIBE_MODULE_DUNDER: &str = "__describe_module__";

/// functions with this prefix run prior to __setup__, initializing global variables and the like
pub const PREINIT_DUNDER: &str = "__preinit__";
/// initializes the user code in the module. fallible
pub const SETUP_DUNDER: &str = "__setup__";
/// the reducer with this name initializes the database
pub const INIT_DUNDER: &str = "__init__";
/// the reducer with this name is invoked when updating the database
pub const UPDATE_DUNDER: &str = "__update__";
/// The reducer with this name is invoked when a client connects.
pub const CLIENT_CONNECTED_DUNDER: &str = "__identity_connected__";
/// The reducer with this name is invoked when a client disconnects.
pub const CLIENT_DISCONNECTED_DUNDER: &str = "__identity_disconnected__";

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(unused)]
pub enum WasmType {
    I32,
    I64,
    F32,
    F64,
    V128,
    FuncRef,
    ExternRef,
}

macro_rules! type_eq {
    ($t:path) => {
        impl PartialEq<WasmType> for $t {
            fn eq(&self, other: &WasmType) -> bool {
                matches!(
                    (self, other),
                    (Self::I32, WasmType::I32)
                        | (Self::I64, WasmType::I64)
                        | (Self::F32, WasmType::F32)
                        | (Self::F64, WasmType::F64)
                        | (Self::V128, WasmType::V128)
                        | (Self::FuncRef, WasmType::FuncRef)
                        | (Self::ExternRef, WasmType::ExternRef)
                )
            }
        }
        impl PartialEq<&WasmType> for $t {
            fn eq(&self, other: &&WasmType) -> bool {
                self.eq(*other)
            }
        }
        impl From<$t> for WasmType {
            fn from(ty: $t) -> WasmType {
                match ty {
                    <$t>::I32 => WasmType::I32,
                    <$t>::I64 => WasmType::I64,
                    <$t>::F32 => WasmType::F32,
                    <$t>::F64 => WasmType::F64,
                    <$t>::V128 => WasmType::V128,
                    <$t>::FuncRef => WasmType::FuncRef,
                    <$t>::ExternRef => WasmType::ExternRef,
                }
            }
        }
    };
}
type_eq!(wasmtime::ValType);

#[derive(Debug)]
pub struct FuncSig<T: AsRef<[WasmType]>> {
    params: T,
    results: T,
}
type StaticFuncSig = FuncSig<&'static [WasmType]>;
type BoxFuncSig = FuncSig<Box<[WasmType]>>;
impl StaticFuncSig {
    const fn new(params: &'static [WasmType], results: &'static [WasmType]) -> Self {
        Self { params, results }
    }
}
impl<T: AsRef<[WasmType]>> PartialEq<FuncSig<T>> for wasmtime::ExternType {
    fn eq(&self, other: &FuncSig<T>) -> bool {
        self.func().map_or(false, |f| {
            f.params().eq(other.params.as_ref()) && f.results().eq(other.results.as_ref())
        })
    }
}
impl FuncSigLike for wasmtime::ExternType {
    fn to_func_sig(&self) -> Option<BoxFuncSig> {
        self.func().map(|f| FuncSig {
            params: f.params().map(Into::into).collect(),
            results: f.results().map(Into::into).collect(),
        })
    }
    fn is_memory(&self) -> bool {
        matches!(self, wasmtime::ExternType::Memory(_))
    }
}

pub trait FuncSigLike: PartialEq<StaticFuncSig> {
    fn to_func_sig(&self) -> Option<BoxFuncSig>;
    fn is_memory(&self) -> bool;
}

const PREINIT_SIG: StaticFuncSig = FuncSig::new(&[], &[]);
const INIT_SIG: StaticFuncSig = FuncSig::new(&[], &[WasmType::I32]);
const DESCRIBE_MODULE_SIG: StaticFuncSig = FuncSig::new(&[], &[WasmType::I32]);
const CALL_REDUCER_SIG: StaticFuncSig = FuncSig::new(
    &[
        WasmType::I32, // Reducer ID
        WasmType::I32, // Sender `Identity` buffer
        WasmType::I32, // Sender `Address` buffer
        WasmType::I64, // Timestamp
        WasmType::I32, // Args buffer
    ],
    &[
        WasmType::I32, // Result buffer
    ],
);

#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    #[error("bad {kind} signature for {name:?}; expected {expected:?} got {actual:?}")]
    MismatchedSignature {
        kind: &'static str,
        name: Box<str>,
        expected: StaticFuncSig,
        actual: BoxFuncSig,
    },
    #[error("expected {name:?} export to be a {kind} with signature {expected:?}, but it wasn't a function at all")]
    NotAFunction {
        kind: &'static str,
        name: Box<str>,
        expected: StaticFuncSig,
    },
    #[error("there should be a memory export called \"memory\" but it does not exist")]
    NoMemory,
    #[error("there should be a function called {name:?} but it does not exist")]
    NoFunction { name: &'static str },
    #[error(transparent)]
    TypeRef(#[from] TypeRefError),
}

#[derive(Default)]
pub struct FuncNames {
    pub preinits: Vec<String>,
}
impl FuncNames {
    fn validate_signature<T>(
        kind: &'static str,
        ty: &T,
        name: &str,
        expected: StaticFuncSig,
    ) -> Result<(), ValidationError>
    where
        T: FuncSigLike,
    {
        if *ty == expected {
            Ok(())
        } else {
            let name = name.into();
            Err(match ty.to_func_sig() {
                Some(actual) => ValidationError::MismatchedSignature {
                    kind,
                    name,
                    expected,
                    actual,
                },
                None => ValidationError::NotAFunction { kind, name, expected },
            })
        }
    }
    pub fn update_from_general<T>(&mut self, sym: &str, ty: &T) -> Result<(), ValidationError>
    where
        T: FuncSigLike,
    {
        if sym == SETUP_DUNDER {
            Self::validate_signature("setup", ty, sym, INIT_SIG)?;
        } else if let Some(name) = sym.strip_prefix(PREINIT_DUNDER) {
            Self::validate_signature("preinit", ty, name, PREINIT_SIG)?;
            self.preinits.push(sym.to_owned());
        }
        Ok(())
    }
    pub fn check_required<F, T>(get_export: F) -> Result<(), ValidationError>
    where
        F: Fn(&str) -> Option<T>,
        T: FuncSigLike,
    {
        get_export("memory")
            .filter(|t| t.is_memory())
            .ok_or(ValidationError::NoMemory)?;

        let get_func = |name| get_export(name).ok_or(ValidationError::NoFunction { name });

        let sig = get_func(CALL_REDUCER_DUNDER)?;
        Self::validate_signature("call_reducer", &sig, CALL_REDUCER_DUNDER, CALL_REDUCER_SIG)?;

        let sig = get_func(DESCRIBE_MODULE_DUNDER)?;
        Self::validate_signature("describe_module", &sig, DESCRIBE_MODULE_DUNDER, DESCRIBE_MODULE_SIG)?;

        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub enum ModuleCreationError {
    WasmCompileError(anyhow::Error),
    Init(#[from] module_host_actor::InitializationError),
    Abi(#[from] abi::AbiVersionError),
}

pub trait ResourceIndex {
    type Resource;
    fn from_u32(i: u32) -> Self;
    fn to_u32(&self) -> u32;
}

macro_rules! decl_index {
    ($name:ident => $resource:ty) => {
        #[derive(Copy, Clone)]
        #[repr(transparent)]
        pub(super) struct $name(pub u32);

        impl ResourceIndex for $name {
            type Resource = $resource;
            fn from_u32(i: u32) -> Self {
                Self(i)
            }
            fn to_u32(&self) -> u32 {
                self.0
            }
        }

        impl $name {
            // for WasmPointee to work in crate::host::wasmtime
            #[allow(unused)]
            #[doc(hidden)]
            pub(super) fn to_le_bytes(self) -> [u8; 4] {
                self.0.to_le_bytes()
            }
            #[allow(unused)]
            #[doc(hidden)]
            pub(super) fn from_le_bytes(b: [u8; 4]) -> Self {
                Self(u32::from_le_bytes(b))
            }
        }
    };
}

pub struct ResourceSlab<I: ResourceIndex> {
    slab: slab::Slab<I::Resource>,
}

impl<I: ResourceIndex> Default for ResourceSlab<I> {
    fn default() -> Self {
        Self {
            slab: slab::Slab::default(),
        }
    }
}

impl<I: ResourceIndex> ResourceSlab<I> {
    pub fn insert(&mut self, data: I::Resource) -> I {
        let idx = self.slab.insert(data) as u32;
        I::from_u32(idx)
    }

    pub fn get(&self, handle: I) -> Option<&I::Resource> {
        self.slab.get(handle.to_u32() as usize)
    }

    pub fn get_mut(&mut self, handle: I) -> Option<&mut I::Resource> {
        self.slab.get_mut(handle.to_u32() as usize)
    }

    pub fn take(&mut self, handle: I) -> Option<I::Resource> {
        self.slab.try_remove(handle.to_u32() as usize)
    }

    pub fn clear(&mut self) {
        self.slab.clear()
    }
}

decl_index!(BufferIdx => bytes::Bytes);
pub(super) type Buffers = ResourceSlab<BufferIdx>;

impl BufferIdx {
    pub const INVALID: Self = Self(u32::MAX);

    pub const fn is_invalid(&self) -> bool {
        self.0 == Self::INVALID.0
    }
}

decl_index!(RowIterIdx => std::vec::IntoIter<Box<[u8]>>);
pub(super) type RowIters = ResourceSlab<RowIterIdx>;

pub(super) struct TimingSpan {
    pub start: Instant,
    pub name: Vec<u8>,
}

impl TimingSpan {
    pub fn new(name: Vec<u8>) -> Self {
        Self {
            start: Instant::now(),
            name,
        }
    }
}

decl_index!(TimingSpanIdx => TimingSpan);
pub(super) type TimingSpanSet = ResourceSlab<TimingSpanIdx>;

pub fn err_to_errno(err: &NodesError) -> Option<NonZeroU16> {
    match err {
        NodesError::TableNotFound => Some(errno::NO_SUCH_TABLE),
        NodesError::ColumnValueNotFound | NodesError::RangeNotFound => Some(errno::LOOKUP_NOT_FOUND),
        NodesError::AlreadyExists(_) => Some(errno::UNIQUE_ALREADY_EXISTS),
        NodesError::Internal(internal) => match **internal {
            DBError::Index(IndexError::UniqueConstraintViolation(UniqueConstraintViolation { .. })) => {
                Some(errno::UNIQUE_ALREADY_EXISTS)
            }
            _ => None,
        },
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
#[error("runtime error calling {func}: {err}")]
pub struct AbiRuntimeError {
    pub func: AbiCall,
    #[source]
    pub err: NodesError,
}

macro_rules! abi_funcs {
    ($mac:ident) => {
        $mac! {
            "spacetime_9.0"::buffer_alloc,
            "spacetime_9.0"::buffer_consume,
            "spacetime_9.0"::buffer_len,
            "spacetime_9.0"::console_log,
            "spacetime_9.0"::create_index,
            "spacetime_9.0"::delete_by_col_eq,
            "spacetime_9.0"::delete_by_rel,
            "spacetime_9.0"::get_table_id,
            "spacetime_9.0"::insert,
            "spacetime_9.0"::iter_by_col_eq,
            "spacetime_9.0"::iter_drop,
            "spacetime_9.0"::iter_advance,
            "spacetime_9.0"::iter_start,
            "spacetime_9.0"::iter_start_filtered,
            "spacetime_9.0"::span_end,
            "spacetime_9.0"::span_start,
        }
    };
}
pub(crate) use abi_funcs;
