// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

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
pub struct UniqueU8 {
    pub n: u8,
    pub data: i32,
}

impl TableType for UniqueU8 {
    const TABLE_NAME: &'static str = "UniqueU8";
    type ReducerEvent = super::ReducerEvent;
}

impl UniqueU8 {
    #[allow(unused)]
    pub fn filter_by_n(n: u8) -> Option<Self> {
        Self::find(|row| row.n == n)
    }
    #[allow(unused)]
    pub fn filter_by_data(data: i32) -> TableIter<Self> {
        Self::filter(|row| row.data == data)
    }
}
