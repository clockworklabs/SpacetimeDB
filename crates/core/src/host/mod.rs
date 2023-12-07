use anyhow::Context;
use bytes::Bytes;
use bytestring::ByteString;
use derive_more::Display;
use enum_map::Enum;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::de::DeserializeSeed;
use spacetimedb_lib::{bsatn, Identity};
use spacetimedb_lib::{ProductValue, ReducerDef};
use spacetimedb_metrics::impl_prometheusvalue_string;
use spacetimedb_metrics::typed_prometheus::AsPrometheusLabel;
use spacetimedb_sats::WithTypespace;
use std::time::Duration;

mod host_controller;
pub(crate) mod module_host;
pub mod scheduler;
mod wasmtime;
// Visible for integration testing.
pub mod instance_env;
mod timestamp;
mod wasm_common;

pub use host_controller::{
    DescribedEntityType, EnergyDiff, EnergyQuanta, HostController, ReducerCallResult, ReducerOutcome, UpdateOutcome,
};
pub use module_host::{ModuleHost, NoSuchModule};
pub use module_host::{UpdateDatabaseResult, UpdateDatabaseSuccess};
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
                bsatn: None,
                json: Some(json),
            },
            ReducerArgs::Bsatn(bytes) => ArgsTuple {
                tuple: ReducerDef::deserialize(schema).deserialize(bsatn::Deserializer::new(&mut &bytes[..]))?,
                bsatn: Some(bytes),
                json: None,
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
    bsatn: Option<Bytes>,
    json: Option<ByteString>,
}

impl ArgsTuple {
    #[allow(clippy::declare_interior_mutable_const)] // false positive on Bytes
    const NULLARY: Self = ArgsTuple {
        tuple: spacetimedb_sats::product![],
        bsatn: Some(Bytes::new()),
        json: Some(ByteString::from_static("[]")),
    };

    pub const fn nullary() -> Self {
        Self::NULLARY
    }

    pub fn get_bsatn(&mut self) -> &Bytes {
        self.bsatn
            .get_or_insert_with(|| bsatn::to_vec(&self.tuple).unwrap().into())
    }
    pub fn get_json(&mut self) -> &ByteString {
        use spacetimedb_sats::ser::serde::SerializeWrapper;
        self.json.get_or_insert_with(|| {
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

pub use module_host::{EntityDef, ReducerCallError};
use spacetimedb_sats::hash::Hash;

fn from_json_seed<'de, T: serde::de::DeserializeSeed<'de>>(s: &'de str, seed: T) -> anyhow::Result<T::Value> {
    let mut de = serde_json::Deserializer::from_str(s);
    let mut track = serde_path_to_error::Track::new();
    let out = seed
        .deserialize(serde_path_to_error::Deserializer::new(&mut de, &mut track))
        .context(track.path())?;
    de.end()?;
    Ok(out)
}

pub struct EnergyMonitorFingerprint<'a> {
    pub module_hash: Hash,
    pub module_identity: Identity,
    pub caller_identity: Identity,
    pub reducer_name: &'a str,
}

pub trait EnergyMonitor: Send + Sync + 'static {
    fn reducer_budget(&self, fingerprint: &EnergyMonitorFingerprint<'_>) -> EnergyQuanta;
    fn record(&self, fingerprint: &EnergyMonitorFingerprint<'_>, energy_used: EnergyDiff, execution_duration: Duration);
}

// what would the module do with this information?
// pub enum EnergyRecordResult {
//     Continue,
//     Exhausted { quanta_over_budget: u64 },
// }

#[derive(Default)]
pub struct NullEnergyMonitor;

impl EnergyMonitor for NullEnergyMonitor {
    fn reducer_budget(&self, _fingerprint: &EnergyMonitorFingerprint<'_>) -> EnergyQuanta {
        EnergyQuanta::DEFAULT_BUDGET
    }

    fn record(
        &self,
        _fingerprint: &EnergyMonitorFingerprint<'_>,
        _energy_used: EnergyDiff,
        _execution_duration: Duration,
    ) {
    }
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
