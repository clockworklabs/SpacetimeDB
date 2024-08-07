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
    Address, ScheduleAt,
};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct UniqueI64 {
    pub n: i64,
    pub data: i32,
}

impl TableType for UniqueI64 {
    const TABLE_NAME: &'static str = "UniqueI64";
    type ReducerEvent = super::ReducerEvent;
}

impl UniqueI64 {
    #[allow(unused)]
    pub fn filter_by_n(n: i64) -> TableIter<Self> {
        Self::filter(|row| row.n == n)
    }
    #[allow(unused)]
    pub fn find_by_n(n: i64) -> Option<Self> {
        Self::find(|row| row.n == n)
    }
    #[allow(unused)]
    pub fn filter_by_data(data: i32) -> TableIter<Self> {
        Self::filter(|row| row.data == data)
    }
}
