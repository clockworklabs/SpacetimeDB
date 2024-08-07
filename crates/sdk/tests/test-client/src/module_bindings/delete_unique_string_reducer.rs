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
pub struct DeleteUniqueStringArgs {
    pub s: String,
}

impl Reducer for DeleteUniqueStringArgs {
    const REDUCER_NAME: &'static str = "delete_unique_string";
}

#[allow(unused)]
pub fn delete_unique_string(s: String) {
    DeleteUniqueStringArgs { s }.invoke();
}

#[allow(unused)]
pub fn on_delete_unique_string(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &String) + Send + 'static,
) -> ReducerCallbackId<DeleteUniqueStringArgs> {
    DeleteUniqueStringArgs::on_reducer(move |__identity, __addr, __status, __args| {
        let DeleteUniqueStringArgs { s } = __args;
        __callback(__identity, __addr, __status, s);
    })
}

#[allow(unused)]
pub fn once_on_delete_unique_string(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &String) + Send + 'static,
) -> ReducerCallbackId<DeleteUniqueStringArgs> {
    DeleteUniqueStringArgs::once_on_reducer(move |__identity, __addr, __status, __args| {
        let DeleteUniqueStringArgs { s } = __args;
        __callback(__identity, __addr, __status, s);
    })
}

#[allow(unused)]
pub fn remove_on_delete_unique_string(id: ReducerCallbackId<DeleteUniqueStringArgs>) {
    DeleteUniqueStringArgs::remove_on_reducer(id);
}
