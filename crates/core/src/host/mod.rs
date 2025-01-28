use anyhow::Context;
use bytes::Bytes;
use bytestring::ByteString;
use derive_more::Display;
use enum_map::Enum;
use once_cell::sync::OnceCell;
use spacetimedb_lib::bsatn;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::de::DeserializeSeed;
use spacetimedb_lib::ProductValue;
use spacetimedb_schema::def::deserialize::ReducerArgsDeserializeSeed;

mod disk_storage;
mod host_controller;
#[allow(clippy::too_many_arguments)]
pub mod module_host;
pub mod scheduler;
pub mod wasmtime;
// Visible for integration testing.
pub mod instance_env;
mod wasm_common;

pub use disk_storage::DiskStorage;
pub use host_controller::{
    DescribedEntityType, DurabilityProvider, ExternalDurability, ExternalStorage, HostController, ProgramStorage,
    ReducerCallResult, ReducerOutcome,
};
pub use module_host::{ModuleHost, NoSuchModule, ReducerCallError, UpdateDatabaseResult};
pub use scheduler::Scheduler;

#[derive(Debug)]
pub enum ReducerArgs {
    Json(ByteString),
    Bsatn(Bytes),
    Nullary,
}

impl ReducerArgs {
    fn into_tuple(self, seed: ReducerArgsDeserializeSeed) -> Result<ArgsTuple, InvalidReducerArguments> {
        self._into_tuple(seed).map_err(|err| InvalidReducerArguments {
            err,
            reducer: (*seed.reducer_def().name).into(),
        })
    }
    fn _into_tuple(self, seed: ReducerArgsDeserializeSeed) -> anyhow::Result<ArgsTuple> {
        Ok(match self {
            ReducerArgs::Json(json) => ArgsTuple {
                tuple: from_json_seed(&json, SeedWrapper(seed))?,
                bsatn: OnceCell::new(),
                json: OnceCell::with_value(json),
            },
            ReducerArgs::Bsatn(bytes) => ArgsTuple {
                tuple: seed.deserialize(bsatn::Deserializer::new(&mut &bytes[..]))?,
                bsatn: OnceCell::with_value(bytes),
                json: OnceCell::new(),
            },
            ReducerArgs::Nullary => {
                anyhow::ensure!(
                    seed.reducer_def().params.elements.is_empty(),
                    "failed to typecheck args"
                );
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
    pub fn nullary() -> Self {
        ArgsTuple {
            tuple: spacetimedb_sats::product![],
            bsatn: OnceCell::with_value(Bytes::new()),
            json: OnceCell::with_value(ByteString::from_static("[]")),
        }
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

// TODO(noa): replace imports from this module with imports straight from primitives.
pub use spacetimedb_primitives::ReducerId;

#[derive(thiserror::Error, Debug)]
#[error("invalid arguments for reducer {reducer}: {err}")]
pub struct InvalidReducerArguments {
    #[source]
    err: anyhow::Error,
    reducer: Box<str>,
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
#[derive(Debug, Display, Enum, Clone, Copy, strum::AsRefStr)]
pub enum AbiCall {
    TableIdFromName,
    IndexIdFromName,
    DatastoreTableRowCount,
    DatastoreTableScanBsatn,
    DatastoreBtreeScanBsatn,
    RowIterBsatnAdvance,
    RowIterBsatnClose,
    DatastoreInsertBsatn,
    DatastoreUpdateBsatn,
    DatastoreDeleteByBtreeScanBsatn,
    DatastoreDeleteAllByEqBsatn,
    BytesSourceRead,
    BytesSinkWrite,
    ConsoleLog,
    ConsoleTimerStart,
    ConsoleTimerEnd,
    Identity,

    VolatileNonatomicScheduleImmediate,
}
