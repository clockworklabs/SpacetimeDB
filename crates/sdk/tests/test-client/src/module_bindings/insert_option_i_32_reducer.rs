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
pub struct InsertOptionI32Args {
    pub n: Option<i32>,
}

impl Reducer for InsertOptionI32Args {
    const REDUCER_NAME: &'static str = "insert_option_i32";
}

#[allow(unused)]
pub fn insert_option_i_32(n: Option<i32>) {
    InsertOptionI32Args { n }.invoke();
}

#[allow(unused)]
pub fn on_insert_option_i_32(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &Option<i32>) + Send + 'static,
) -> ReducerCallbackId<InsertOptionI32Args> {
    InsertOptionI32Args::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOptionI32Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_insert_option_i_32(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &Option<i32>) + Send + 'static,
) -> ReducerCallbackId<InsertOptionI32Args> {
    InsertOptionI32Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOptionI32Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_insert_option_i_32(id: ReducerCallbackId<InsertOptionI32Args>) {
    InsertOptionI32Args::remove_on_reducer(id);
}
