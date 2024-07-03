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
    Address,
};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct InsertOptionStringArgs {
    pub s: Option<String>,
}

impl Reducer for InsertOptionStringArgs {
    const REDUCER_NAME: &'static str = "insert_option_string";
}

#[allow(unused)]
pub fn insert_option_string(s: Option<String>) {
    InsertOptionStringArgs { s }.invoke();
}

#[allow(unused)]
pub fn on_insert_option_string(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &Option<String>) + Send + 'static,
) -> ReducerCallbackId<InsertOptionStringArgs> {
    InsertOptionStringArgs::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOptionStringArgs { s } = __args;
        __callback(__identity, __addr, __status, s);
    })
}

#[allow(unused)]
pub fn once_on_insert_option_string(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &Option<String>) + Send + 'static,
) -> ReducerCallbackId<InsertOptionStringArgs> {
    InsertOptionStringArgs::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOptionStringArgs { s } = __args;
        __callback(__identity, __addr, __status, s);
    })
}

#[allow(unused)]
pub fn remove_on_insert_option_string(id: ReducerCallbackId<InsertOptionStringArgs>) {
    InsertOptionStringArgs::remove_on_reducer(id);
}
