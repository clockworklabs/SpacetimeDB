pub mod abi;
pub mod host_actor;

use std::fmt;

use anyhow::anyhow;
use spacetimedb_lib::EntityDef;

use super::host_controller::ReducerBudget;

pub const REDUCE_DUNDER: &str = "__reducer__";
pub const DESCRIBE_REDUCER_DUNDER: &str = "__describe_reducer__";

pub const DESCRIBE_TABLE_DUNDER: &str = "__describe_table__";

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

pub const DEFAULT_EXECUTION_BUDGET: ReducerBudget = ReducerBudget(1_000_000_000_000_000);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(unused)]
enum WasmType {
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
    };
}
type_eq!(wasmer::Type);

#[derive(Debug)]
pub struct FuncSig<'a> {
    params: &'a [WasmType],
    results: &'a [WasmType],
}
impl<'a> FuncSig<'a> {
    const fn new(params: &'a [WasmType], results: &'a [WasmType]) -> Self {
        Self { params, results }
    }
}
impl PartialEq<FuncSig<'_>> for wasmer::FunctionType {
    fn eq(&self, other: &FuncSig<'_>) -> bool {
        self.params().eq(other.params) && self.results().eq(other.results)
    }
}
impl PartialEq<FuncSig<'_>> for wasmer::ExternType {
    fn eq(&self, other: &FuncSig<'_>) -> bool {
        matches!(self, wasmer::ExternType::Function(f) if f == other)
    }
}

const PREINIT_SIG: FuncSig = FuncSig::new(&[], &[]);
const INIT_SIG: FuncSig = FuncSig::new(&[], &[WasmType::I32]);
const REDUCER_SIG: FuncSig = FuncSig::new(&[WasmType::I32, WasmType::I64, WasmType::I32], &[WasmType::I32]);

#[derive(Default)]
pub struct FuncNames {
    // pub reducers: IndexMap<String, String>,
    pub migrates: Vec<String>,
    pub conn: bool,
    pub disconn: bool,
    pub preinits: Vec<String>,
}
impl FuncNames {
    fn validate_signature<T>(kind: &str, ty: &T, name: &str, expected: FuncSig<'_>) -> anyhow::Result<()>
    where
        for<'a> T: PartialEq<FuncSig<'a>> + fmt::Debug,
    {
        anyhow::ensure!(
            *ty == expected,
            "bad {kind} signature for {name:?}; expected {expected:?} got {ty:?}",
        );
        Ok(())
    }
    pub fn update_from_entity<F, T>(&mut self, get_export: F, name: &str, entity: &EntityDef) -> anyhow::Result<()>
    where
        F: Fn(&str) -> Option<T>,
        for<'a> T: PartialEq<FuncSig<'a>> + fmt::Debug,
    {
        let check_signature = |kind, func_name, expected| {
            let ty = get_export(func_name)
                .ok_or_else(|| anyhow!("a descriptor exists for {kind} {name:?} but not a {func_name:?} function"))?;
            Self::validate_signature(kind, &ty, name, expected)
        };
        match &entity {
            EntityDef::Reducer(_) => {
                check_signature("reducer", &[REDUCE_DUNDER, name].concat(), REDUCER_SIG)?;
            }
            EntityDef::Table(_) => {}
        }
        Ok(())
    }
    pub fn update_from_general<T>(&mut self, sym: &str, ty: &T) -> anyhow::Result<()>
    where
        for<'a> T: PartialEq<FuncSig<'a>> + fmt::Debug,
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
