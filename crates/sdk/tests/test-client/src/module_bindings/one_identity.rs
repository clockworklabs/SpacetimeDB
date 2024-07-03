// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused_imports)]
use spacetimedb_sdk::{
    anyhow::{anyhow, Result},
    identity::Identity,
    reducer::{Reducer, ReducerCallbackId, Status},
    sats::{de::Deserialize, i256, ser::Serialize, u256},
    spacetimedb_lib,
    table::{TableIter, TableType, TableWithPrimaryKey},
    Address,
};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct OneIdentity {
    pub i: Identity,
}

impl TableType for OneIdentity {
    const TABLE_NAME: &'static str = "OneIdentity";
    type ReducerEvent = super::ReducerEvent;
}

impl OneIdentity {
    #[allow(unused)]
    pub fn filter_by_i(i: Identity) -> TableIter<Self> {
        Self::filter(|row| row.i == i)
    }
}
