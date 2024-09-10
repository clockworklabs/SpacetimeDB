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
pub struct OptionVecOptionI32 {
    pub v: Option<Vec<Option<i32>>>,
}

impl TableType for OptionVecOptionI32 {
    const TABLE_NAME: &'static str = "option_vec_option_i32";
    type ReducerEvent = super::ReducerEvent;
}

impl OptionVecOptionI32 {}
