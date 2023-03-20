use anyhow::Context;
use bytes::Bytes;
use bytestring::ByteString;
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
    Json(ByteString),
    Bsatn(Bytes),
    Nullary,
}

impl ReducerArgs {
    fn into_tuple(self, schema: TypeInSpace<'_, ReducerDef>) -> Result<ArgsTuple, InvalidReducerArguments> {
        self._into_tuple(schema).map_err(|err| InvalidReducerArguments {
            err,
            reducer: schema.ty().name.as_deref().unwrap_or("").to_owned(),
        })
    }
    fn _into_tuple(self, schema: TypeInSpace<'_, ReducerDef>) -> anyhow::Result<ArgsTuple> {
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
                ArgsTuple::default()
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct ArgsTuple {
    tuple: TupleValue,
    bsatn: Option<Bytes>,
    json: Option<ByteString>,
}

impl ArgsTuple {
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
        Self {
            tuple: spacetimedb_sats::product![],
            bsatn: Some(Bytes::new()),
            json: Some("[]".into()),
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

fn from_json_seed<'de, T: serde::de::DeserializeSeed<'de>>(s: &'de str, seed: T) -> anyhow::Result<T::Value> {
    let mut de = serde_json::Deserializer::from_str(s);
    let mut track = serde_path_to_error::Track::new();
    let out = seed
        .deserialize(serde_path_to_error::Deserializer::new(&mut de, &mut track))
        .context(track.path())?;
    de.end()?;
    Ok(out)
}
