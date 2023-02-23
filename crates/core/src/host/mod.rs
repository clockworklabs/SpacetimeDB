use bytes::Bytes;
use spacetimedb_lib::bsatn;
use spacetimedb_lib::de::serde::SeedWrapper;
use spacetimedb_lib::de::DeserializeSeed;
use spacetimedb_lib::{ReducerDef, TupleValue};
use spacetimedb_sats::TypeInSpace;

pub mod host_controller;
mod host_wasmer;
pub(crate) mod module_host;

// Visible for integration testing.
pub mod instance_env;
mod timestamp;
pub mod tracelog;
mod wasm_common;

#[derive(Debug)]
pub enum ReducerArgs {
    Json(Bytes),
    Bsatn(Bytes),
    Nullary,
}

impl ReducerArgs {
    fn into_tuple(self, schema: TypeInSpace<'_, ReducerDef>) -> Result<TupleValue, InvalidReducerArguments> {
        self._into_tuple(schema).map_err(|err| InvalidReducerArguments {
            err,
            reducer: schema.ty().name.as_deref().unwrap_or("").to_owned(),
        })
    }
    fn _into_tuple(self, schema: TypeInSpace<'_, ReducerDef>) -> anyhow::Result<TupleValue> {
        match self {
            ReducerArgs::Json(json) => {
                let args = from_json_seed(&json, SeedWrapper(ReducerDef::deserialize(schema)))?;
                Ok(args)
            }
            ReducerArgs::Bsatn(bytes) => {
                Ok(ReducerDef::deserialize(schema).deserialize(bsatn::Deserializer::new(&mut &bytes[..]))?)
            }
            ReducerArgs::Nullary => {
                anyhow::ensure!(schema.ty().args.is_empty(), "failed to typecheck args");
                Ok(TupleValue { elements: vec![] })
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[error("invalid arguments for reducer {reducer}")]
pub struct InvalidReducerArguments {
    #[source]
    err: anyhow::Error,
    reducer: String,
}

pub use module_host::ReducerCallError;

fn from_json_seed<'de, T: serde::de::DeserializeSeed<'de>>(
    s: &'de [u8],
    seed: T,
) -> Result<T::Value, serde_json::Error> {
    let mut de = serde_json::Deserializer::from_slice(s);
    let out = seed.deserialize(&mut de)?;
    de.end()?;
    Ok(out)
}
