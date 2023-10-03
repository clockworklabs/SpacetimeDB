// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#[allow(unused)]
use spacetimedb_sdk::{
    anyhow::{anyhow, Result},
    identity::Identity,
    reducer::{Reducer, ReducerCallbackId, Status},
    sats::{de::Deserialize, ser::Serialize},
    spacetimedb_lib,
    table::{TableIter, TableType, TableWithPrimaryKey},
    Address,
};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Disconnected {
    pub identity: Identity,
}

impl TableType for Disconnected {
    const TABLE_NAME: &'static str = "Disconnected";
    type ReducerEvent = super::ReducerEvent;
}

impl Disconnected {
    #[allow(unused)]
    pub fn filter_by_identity(identity: Identity) -> TableIter<Self> {
        Self::filter(|row| row.identity == identity)
    }
}
