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
pub struct UniqueAddress {
    pub a: Address,
    pub data: i32,
}

impl TableType for UniqueAddress {
    const TABLE_NAME: &'static str = "UniqueAddress";
    type ReducerEvent = super::ReducerEvent;
}

impl UniqueAddress {
    #[allow(unused)]
    pub fn filter_by_a(a: Address) -> TableIter<Self> {
        Self::filter(|row| row.a == a)
    }
    #[allow(unused)]
    pub fn find_by_a(a: Address) -> Option<Self> {
        Self::find(|row| row.a == a)
    }
    #[allow(unused)]
    pub fn filter_by_data(data: i32) -> TableIter<Self> {
        Self::filter(|row| row.data == data)
    }
}
