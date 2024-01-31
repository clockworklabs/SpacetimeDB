use anyhow::Context;
use bytes::Bytes;
use bytestring::ByteString;
use derive_more::Display;
use enum_map::Enum;
use once_cell::sync::OnceCell;
use spacetimedb_lib::bsatn;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::de::DeserializeSeed;
use spacetimedb_lib::{ProductValue, ReducerDef};
use spacetimedb_metrics::impl_prometheusvalue_string;
use spacetimedb_metrics::typed_prometheus::AsPrometheusLabel;
use spacetimedb_sats::WithTypespace;

mod host_controller;
pub(crate) mod module_host;
pub mod scheduler;
mod wasmtime;
// Visible for integration testing.
pub mod instance_env;
mod timestamp;
mod wasm_common;

pub use host_controller::{DescribedEntityType, HostController, ReducerCallResult, ReducerOutcome, UpdateOutcome};
pub use module_host::{
    EntityDef, ModuleHost, NoSuchModule, ReducerCallError, UpdateDatabaseResult, UpdateDatabaseSuccess,
};
pub use scheduler::Scheduler;
pub use timestamp::Timestamp;

#[derive(Debug)]
pub enum ReducerArgs {
    Json(ByteString),
    Bsatn(Bytes),
    Nullary,
}

impl ReducerArgs {
    fn into_tuple(self, schema: WithTypespace<'_, ReducerDef>) -> Result<ArgsTuple, InvalidReducerArguments> {
        self._into_tuple(schema).map_err(|err| InvalidReducerArguments {
            err,
            reducer: schema.ty().name.clone(),
        })
    }
    fn _into_tuple(self, schema: WithTypespace<'_, ReducerDef>) -> anyhow::Result<ArgsTuple> {
        Ok(match self {
            ReducerArgs::Json(json) => ArgsTuple {
                tuple: from_json_seed(&json, SeedWrapper(ReducerDef::deserialize(schema)))?,
                bsatn: OnceCell::new(),
                json: OnceCell::with_value(json),
            },
            ReducerArgs::Bsatn(bytes) => ArgsTuple {
                tuple: ReducerDef::deserialize(schema).deserialize(bsatn::Deserializer::new(&mut &bytes[..]))?,
                bsatn: OnceCell::with_value(bytes),
                json: OnceCell::new(),
            },
            ReducerArgs::Nullary => {
                anyhow::ensure!(schema.ty().args.is_empty(), "failed to typecheck args");
                ArgsTuple::nullary()
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct ArgsTuple {
    tuple: ProductValue,
    bsatn: OnceCell<Bytes>,
    json: OnceCell<ByteString>,
}

impl ArgsTuple {
    #[allow(clippy::declare_interior_mutable_const)] // false positive on Bytes
    const NULLARY: Self = ArgsTuple {
        tuple: spacetimedb_sats::product![],
        bsatn: OnceCell::with_value(Bytes::new()),
        json: OnceCell::with_value(ByteString::from_static("[]")),
    };

    pub const fn nullary() -> Self {
        Self::NULLARY
    }

    pub fn get_bsatn(&self) -> &Bytes {
        self.bsatn.get_or_init(|| bsatn::to_vec(&self.tuple).unwrap().into())
    }
    pub fn get_json(&self) -> &ByteString {
        use spacetimedb_sats::ser::serde::SerializeWrapper;
        self.json.get_or_init(|| {
            serde_json::to_string(SerializeWrapper::from_ref(&self.tuple))
                .unwrap()
                .into()
        })
    }
}

impl Default for ArgsTuple {
    fn default() -> Self {
        Self::nullary()
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct ReducerId(u32);
impl std::fmt::Display for ReducerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl From<usize> for ReducerId {
    fn from(id: usize) -> Self {
        Self(id as u32)
    }
}

#[derive(thiserror::Error, Debug)]
#[error("invalid arguments for reducer {reducer}: {err}")]
pub struct InvalidReducerArguments {
    #[source]
    err: anyhow::Error,
    reducer: String,
}

fn from_json_seed<'de, T: serde::de::DeserializeSeed<'de>>(s: &'de str, seed: T) -> anyhow::Result<T::Value> {
    let mut de = serde_json::Deserializer::from_str(s);
    let mut track = serde_path_to_error::Track::new();
    let out = seed
        .deserialize(serde_path_to_error::Deserializer::new(&mut de, &mut track))
        .context(track.path())?;
    de.end()?;
    Ok(out)
}

/// Tags for each call that a `WasmInstanceEnv` can make.
#[derive(Debug, Display, Enum, Clone, Copy)]
pub enum AbiCall {
    CancelReducer,
    ConsoleLog,
    CreateIndex,
    DeleteByColEq,
    DeleteByRel,
    GetTableId,
    Insert,
    IterByColEq,
    IterDrop,
    IterNext,
    IterStart,
    IterStartFiltered,
    ScheduleReducer,
}

impl_prometheusvalue_string!(AbiCall);
