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
pub struct OneBool {
    pub b: bool,
}

impl TableType for OneBool {
    const TABLE_NAME: &'static str = "OneBool";
    type ReducerEvent = super::ReducerEvent;
}

impl OneBool {
    #[allow(unused)]
    pub fn filter_by_b(b: bool) -> TableIter<Self> {
        Self::filter(|row| row.b == b)
    }
}
