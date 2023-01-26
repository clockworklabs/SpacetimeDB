pub mod abi;
pub mod host_actor;

use spacetimedb_lib::EntityDef;

use crate::error::NodesError;

pub const REDUCE_DUNDER: &str = "__reducer__";
pub const DESCRIBE_REDUCER_DUNDER: &str = "__describe_reducer__";

pub const DESCRIBE_TABLE_DUNDER: &str = "__describe_table__";
pub const DESCRIBE_TYPESPACE: &str = "__describe_typespace__";

/// functions with this prefix run prior to __setup__, initializing global variables and the like
pub const PREINIT_DUNDER: &str = "__preinit__";
/// initializes the user code in the module. fallible
pub const SETUP_DUNDER: &str = "__setup__";
/// the reducer with this name initializes the database
pub const INIT_DUNDER: &str = "__init__";
pub const MIGRATE_DATABASE_DUNDER: &str = "__migrate_database__";
pub const IDENTITY_CONNECTED_DUNDER: &str = "__identity_connected__";
pub const IDENTITY_DISCONNECTED_DUNDER: &str = "__identity_disconnected__";

pub const STDB_ABI_SYM: &str = "SPACETIME_ABI_VERSION";
pub const STDB_ABI_IS_ADDR_SYM: &str = "SPACETIME_ABI_VERSION_IS_ADDR";

pub const DEFAULT_EXECUTION_BUDGET: i64 = 1_000_000_000_000_000_000;

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
type_eq!(wasmer::Type);

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
impl<T: AsRef<[WasmType]>> PartialEq<FuncSig<T>> for wasmer::ExternType {
    fn eq(&self, other: &FuncSig<T>) -> bool {
        self.func().map_or(false, |f| {
            f.params() == other.params.as_ref() && f.results() == other.results.as_ref()
        })
    }
}
impl FuncSigLike for wasmer::ExternType {
    fn to_func_sig(&self) -> Option<BoxFuncSig> {
        self.func().map(|f| FuncSig {
            params: f.params().iter().map(|t| (*t).into()).collect(),
            results: f.results().iter().map(|t| (*t).into()).collect(),
        })
    }
    fn is_memory(&self) -> bool {
        matches!(self, wasmer::ExternType::Memory(_))
    }
}

pub trait FuncSigLike: PartialEq<StaticFuncSig> {
    fn to_func_sig(&self) -> Option<BoxFuncSig>;
    fn is_memory(&self) -> bool;
}

const PREINIT_SIG: StaticFuncSig = FuncSig::new(&[], &[]);
const INIT_SIG: StaticFuncSig = FuncSig::new(&[], &[WasmType::I32]);
const REDUCER_SIG: StaticFuncSig = FuncSig::new(&[WasmType::I32, WasmType::I64, WasmType::I32], &[WasmType::I32]);

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
    #[error("a descriptor exists for {kind} {name:?} but not a {func_name:?} function")]
    NoExport {
        kind: &'static str,
        name: Box<str>,
        func_name: Box<str>,
    },
    #[error("there should be a memory export called \"memory\" but it does not exist")]
    NoMemory,
}

#[derive(Default)]
pub struct FuncNames {
    // pub reducers: IndexMap<String, String>,
    pub migrates: Vec<String>,
    pub conn: bool,
    pub disconn: bool,
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
    pub fn update_from_entity<F, T>(
        &mut self,
        get_export: F,
        name: &str,
        entity: &EntityDef,
    ) -> Result<(), ValidationError>
    where
        F: Fn(&str) -> Option<T>,
        T: FuncSigLike,
    {
        let check_signature = |kind, func_name: String, expected| {
            let ty = get_export(&func_name).ok_or_else(|| ValidationError::NoExport {
                kind,
                name: name.into(),
                func_name: func_name.into(),
            })?;
            Self::validate_signature(kind, &ty, name, expected)
        };
        match &entity {
            EntityDef::Reducer(_) => {
                check_signature("reducer", [REDUCE_DUNDER, name].concat(), REDUCER_SIG)?;
            }
            EntityDef::Table(_) => {}
        }
        Ok(())
    }
    pub fn update_from_general<T>(&mut self, sym: &str, ty: &T) -> Result<(), ValidationError>
    where
        T: FuncSigLike,
    {
        if let Some(name) = sym.strip_prefix(MIGRATE_DATABASE_DUNDER) {
            Self::validate_signature("migrate", ty, name, REDUCER_SIG)?;
            self.migrates.push(sym.to_owned());
        } else if sym == IDENTITY_CONNECTED_DUNDER {
            self.conn = true;
        } else if sym == IDENTITY_DISCONNECTED_DUNDER {
            self.disconn = true;
        } else if sym == SETUP_DUNDER {
            Self::validate_signature("setup", ty, sym, INIT_SIG)?;
        } else if let Some(name) = sym.strip_prefix(PREINIT_DUNDER) {
            Self::validate_signature("preinit", ty, name, PREINIT_SIG)?;
            self.preinits.push(sym.to_owned());
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub enum ModuleCreationError {
    WasmCompileError(anyhow::Error),
    Abi(#[from] abi::AbiVersionError),
    Init(#[from] host_actor::InitializationError),
}

pub trait ResourceIndex {
    type Resource;
    fn from_u32(i: u32) -> Self;
    fn to_u32(&self) -> u32;
}

macro_rules! decl_index {
    ($name:ident => $resource:ty) => {
        #[derive(Copy, Clone, wasmer::ValueType)]
        #[repr(transparent)]
        pub struct $name(pub u32);

        impl ResourceIndex for $name {
            type Resource = $resource;
            fn from_u32(i: u32) -> Self {
                Self(i)
            }
            fn to_u32(&self) -> u32 {
                self.0
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

decl_index!(BufferIdx => Vec<u8>);
pub type Buffers = ResourceSlab<BufferIdx>;

impl BufferIdx {
    pub const INVALID: Self = Self(u32::MAX);

    pub const fn is_invalid(&self) -> bool {
        self.0 == Self::INVALID.0
    }
}

decl_index!(BufferIterIdx => Box<dyn Iterator<Item = Result<Vec<u8>, NodesError>> + Send>);
pub type BufferIters = ResourceSlab<BufferIterIdx>;

pub mod errnos {
    include!("../../../bindings-sys/src/errno.rs");

    macro_rules! nothing {
        ($($tt:tt)*) => {};
    }
    errnos!(nothing);
}

pub fn err_to_errno(err: &NodesError) -> Option<u16> {
    match err {
        NodesError::TableNotFound => Some(errnos::NOTAB),
        NodesError::PrimaryKeyNotFound(_) => Some(errnos::LOOKUP),
        NodesError::ColumnValueNotFound => Some(errnos::LOOKUP),
        NodesError::RangeNotFound => Some(errnos::LOOKUP),
        NodesError::AlreadyExists(_) => Some(errnos::EXISTS),
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
#[error("runtime error calling {func}: {err}")]
pub struct AbiRuntimeError {
    pub func: &'static str,
    #[source]
    pub err: NodesError,
}
