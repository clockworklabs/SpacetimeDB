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
pub struct OneAddress {
    pub a: Address,
}

impl TableType for OneAddress {
    const TABLE_NAME: &'static str = "OneAddress";
    type ReducerEvent = super::ReducerEvent;
}

impl OneAddress {
    #[allow(unused)]
    pub fn filter_by_a(a: Address) -> TableIter<Self> {
        Self::filter(|row| row.a == a)
    }
}
