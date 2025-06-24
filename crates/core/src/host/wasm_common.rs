pub mod abi;
pub mod instrumentation;
pub mod module_host_actor;

use std::fmt;
use std::num::NonZeroU16;
use std::time::Instant;

use super::{scheduler::ScheduleError, AbiCall};
use crate::error::{DBError, DatastoreError, IndexError, NodesError};
use spacetimedb_primitives::errno;
use spacetimedb_sats::typespace::TypeRefError;
use spacetimedb_table::table::UniqueConstraintViolation;

pub const CALL_REDUCER_DUNDER: &str = "__call_reducer__";

pub const DESCRIBE_MODULE_DUNDER: &str = "__describe_module__";

/// functions with this prefix run prior to __setup__, initializing global variables and the like
pub const PREINIT_DUNDER: &str = "__preinit__";
/// initializes the user code in the module. fallible
pub const SETUP_DUNDER: &str = "__setup__";

#[derive(Debug, Clone)]
#[allow(unused)]
pub enum WasmType {
    I32,
    I64,
    F32,
    F64,
    V128,
    #[allow(clippy::box_collection)]
    Ref(Box<String>),
}

impl fmt::Display for WasmType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            WasmType::I32 => "i32",
            WasmType::I64 => "i64",
            WasmType::F32 => "f32",
            WasmType::F64 => "f64",
            WasmType::V128 => "v128",
            WasmType::Ref(r) => r,
        })
    }
}

impl PartialEq<WasmType> for wasmtime::ValType {
    fn eq(&self, other: &WasmType) -> bool {
        matches!(
            (self, other),
            (Self::I32, WasmType::I32)
                | (Self::I64, WasmType::I64)
                | (Self::F32, WasmType::F32)
                | (Self::F64, WasmType::F64)
                | (Self::V128, WasmType::V128)
        )
    }
}
impl PartialEq<&WasmType> for wasmtime::ValType {
    fn eq(&self, other: &&WasmType) -> bool {
        self.eq(*other)
    }
}
impl From<wasmtime::ValType> for WasmType {
    fn from(ty: wasmtime::ValType) -> WasmType {
        match ty {
            wasmtime::ValType::I32 => WasmType::I32,
            wasmtime::ValType::I64 => WasmType::I64,
            wasmtime::ValType::F32 => WasmType::F32,
            wasmtime::ValType::F64 => WasmType::F64,
            wasmtime::ValType::V128 => WasmType::V128,
            wasmtime::ValType::Ref(ty) => WasmType::Ref(Box::new(ty.to_string())),
        }
    }
}

#[derive(Debug)]
pub struct FuncSig<T: AsRef<[WasmType]>> {
    params: T,
    results: T,
}
impl<T: AsRef<[WasmType]>> fmt::Display for FuncSig<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(func")?;
        let (params, results) = (self.params.as_ref(), self.results.as_ref());
        if !params.is_empty() {
            write!(f, " (param")?;
            for p in params {
                write!(f, " {p}")?;
            }
            write!(f, ")")?;
        }
        if !results.is_empty() {
            write!(f, " (result")?;
            for r in results {
                write!(f, " {r}")?;
            }
            write!(f, ")")?;
        }
        write!(f, ")")
    }
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
        self.func()
            .is_some_and(|f| f.params().eq(other.params.as_ref()) && f.results().eq(other.results.as_ref()))
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
const INIT_SIG: StaticFuncSig = FuncSig::new(&[WasmType::I32], &[WasmType::I32]);
const DESCRIBE_MODULE_SIG: StaticFuncSig = FuncSig::new(&[WasmType::I32], &[]);
const CALL_REDUCER_SIG: StaticFuncSig = FuncSig::new(
    &[
        WasmType::I32, // Reducer ID
        // Sender's `Identity` broken into 4 u64s.
        // ----------------------------------------------------
        WasmType::I64, // `sender_0` contains bytes `[0 ..8 ]`.
        WasmType::I64, // `sender_1` contains bytes `[8 ..16]`.
        WasmType::I64, // `sender_1` contains bytes `[16..24]`.
        WasmType::I64, // `sender_1` contains bytes `[24..32]`.
        // ----------------------------------------------------
        // Caller's `ConnectionId` broken into 2 u64s.
        // ----------------------------------------------------
        WasmType::I64, // `conn_id_0` contains bytes `[0..8 ]`.
        WasmType::I64, // `conn_id_1` contains bytes `[8..16]`.
        // ----------------------------------------------------
        WasmType::I64, // Timestamp
        WasmType::I32, // Args source buffer
        WasmType::I32, // Errors sink buffer
    ],
    &[
        WasmType::I32, // Result code
    ],
);

#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    #[error("bad {kind} signature for {name:?}; expected {expected} got {actual}")]
    MismatchedSignature {
        kind: &'static str,
        name: Box<str>,
        expected: StaticFuncSig,
        actual: BoxFuncSig,
    },
    #[error("expected {name:?} export to be a {kind} with signature {expected}, but it wasn't a function at all")]
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
                Self(i + 1)
            }
            fn to_u32(&self) -> u32 {
                self.0 - 1
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

    pub fn get_mut(&mut self, handle: I) -> Option<&mut I::Resource> {
        self.slab.get_mut(handle.to_u32() as usize)
    }

    pub fn take(&mut self, handle: I) -> Option<I::Resource> {
        self.slab.try_remove(handle.to_u32() as usize)
    }
}

decl_index!(RowIterIdx => std::vec::IntoIter<Vec<u8>>);
pub(super) type RowIters = ResourceSlab<RowIterIdx>;

pub(super) struct TimingSpan {
    pub start: Instant,
    pub name: String,
}

impl TimingSpan {
    pub fn new(name: String) -> Self {
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
        NodesError::NotInTransaction => Some(errno::NOT_IN_TRANSACTION),
        NodesError::DecodeRow(_) => Some(errno::BSATN_DECODE_ERROR),
        NodesError::TableNotFound => Some(errno::NO_SUCH_TABLE),
        NodesError::IndexNotFound => Some(errno::NO_SUCH_INDEX),
        NodesError::IndexNotUnique => Some(errno::INDEX_NOT_UNIQUE),
        NodesError::IndexRowNotFound => Some(errno::NO_SUCH_ROW),
        NodesError::ScheduleError(ScheduleError::DelayTooLong(_)) => Some(errno::SCHEDULE_AT_DELAY_TOO_LONG),
        NodesError::AlreadyExists(_) => Some(errno::UNIQUE_ALREADY_EXISTS),
        NodesError::Internal(internal) => match **internal {
            DBError::Datastore(DatastoreError::Index(IndexError::UniqueConstraintViolation(
                UniqueConstraintViolation {
                    constraint_name: _,
                    table_name: _,
                    cols: _,
                    value: _,
                },
            ))) => Some(errno::UNIQUE_ALREADY_EXISTS),
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
            "spacetime_10.0"::table_id_from_name,
            "spacetime_10.0"::datastore_table_row_count,
            "spacetime_10.0"::datastore_table_scan_bsatn,
            "spacetime_10.0"::row_iter_bsatn_advance,
            "spacetime_10.0"::row_iter_bsatn_close,
            "spacetime_10.0"::datastore_insert_bsatn,
            "spacetime_10.0"::datastore_update_bsatn,
            "spacetime_10.0"::datastore_delete_all_by_eq_bsatn,
            "spacetime_10.0"::bytes_source_read,
            "spacetime_10.0"::bytes_sink_write,
            "spacetime_10.0"::console_log,
            "spacetime_10.0"::console_timer_start,
            "spacetime_10.0"::console_timer_end,
            "spacetime_10.0"::index_id_from_name,
            "spacetime_10.0"::datastore_index_scan_range_bsatn,
            "spacetime_10.0"::datastore_delete_by_index_scan_range_bsatn,
            "spacetime_10.0"::datastore_btree_scan_bsatn,
            "spacetime_10.0"::datastore_delete_by_btree_scan_bsatn,
            "spacetime_10.0"::identity,

            // unstable:
            "spacetime_10.0"::volatile_nonatomic_schedule_immediate,
        }
    };
}
pub(crate) use abi_funcs;
