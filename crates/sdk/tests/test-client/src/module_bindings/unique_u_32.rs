// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused_imports)]
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
pub struct UniqueU32 {
    pub n: u32,
    pub data: i32,
}

impl TableType for UniqueU32 {
    const TABLE_NAME: &'static str = "UniqueU32";
    type ReducerEvent = super::ReducerEvent;
}

impl UniqueU32 {
    #[allow(unused)]
    pub fn filter_by_n(n: u32) -> Option<Self> {
        Self::find(|row| row.n == n)
    }
    #[allow(unused)]
    pub fn filter_by_data(data: i32) -> TableIter<Self> {
        Self::filter(|row| row.data == data)
    }
}
