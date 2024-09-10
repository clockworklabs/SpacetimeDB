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
pub struct OneU16 {
    pub n: u16,
}

impl TableType for OneU16 {
    const TABLE_NAME: &'static str = "one_u16";
    type ReducerEvent = super::ReducerEvent;
}

impl OneU16 {
    #[allow(unused)]
    pub fn filter_by_n(n: u16) -> TableIter<Self> {
        Self::filter(|row| row.n == n)
    }
}
