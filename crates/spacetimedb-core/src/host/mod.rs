use anyhow::Context;
use bytes::Bytes;
use spacetimedb_lib::{ReducerDef, TupleValue};

pub mod host_controller;
mod host_wasmer;
pub(crate) mod module_host;

// Visible for integration testing.
pub mod instance_env;
pub mod tracelog;
mod wasm_common;

#[derive(Debug)]
pub enum ReducerArgs {
    Json(Bytes),
}

impl ReducerArgs {
    fn into_tuple(self, schema: &ReducerDef) -> anyhow::Result<TupleValue> {
        self._into_tuple(schema).with_context(|| InvalidReducerArguments {
            reducer: schema.name.as_deref().unwrap_or("").to_owned(),
        })
    }
    fn _into_tuple(self, schema: &ReducerDef) -> anyhow::Result<TupleValue> {
        match self {
            ReducerArgs::Json(json) => {
                use serde::de::DeserializeSeed;
                let mut de = serde_json::Deserializer::from_slice(&json);
                let args = schema.deserialize(&mut de)?;
                de.end()?;
                Ok(args)
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[error("invalid arguments for reducer {reducer}")]
pub struct InvalidReducerArguments {
    reducer: String,
}
