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
pub struct OneU128 {
    pub n: u128,
}

impl TableType for OneU128 {
    const TABLE_NAME: &'static str = "OneU128";
    type ReducerEvent = super::ReducerEvent;
}

impl OneU128 {
    #[allow(unused)]
    pub fn filter_by_n(n: u128) -> TableIter<Self> {
        Self::filter(|row| row.n == n)
    }
}
