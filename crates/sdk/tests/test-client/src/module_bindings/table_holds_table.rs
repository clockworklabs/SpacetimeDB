// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused_imports)]
use super::one_u_8::OneU8;
use super::vec_u_8::VecU8;
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
pub struct TableHoldsTable {
    pub a: OneU8,
    pub b: VecU8,
}

impl TableType for TableHoldsTable {
    const TABLE_NAME: &'static str = "table_holds_table";
    type ReducerEvent = super::ReducerEvent;
}

impl TableHoldsTable {}
