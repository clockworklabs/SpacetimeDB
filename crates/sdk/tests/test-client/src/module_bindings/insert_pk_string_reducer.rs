// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#[allow(unused)]
use spacetimedb_sdk::{
    anyhow::{anyhow, Result},
    identity::Identity,
    reducer::{Reducer, ReducerCallbackId, Status},
    sats::{de::Deserialize, ser::Serialize},
    spacetimedb_lib,
    table::{TableIter, TableType, TableWithPrimaryKey},
    Address,
};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct InsertPkStringArgs {
    pub s: String,
    pub data: i32,
}

impl Reducer for InsertPkStringArgs {
    const REDUCER_NAME: &'static str = "insert_pk_string";
}

#[allow(unused)]
pub fn insert_pk_string(s: String, data: i32) {
    InsertPkStringArgs { s, data }.invoke();
}

#[allow(unused)]
pub fn on_insert_pk_string(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &String, &i32) + Send + 'static,
) -> ReducerCallbackId<InsertPkStringArgs> {
    InsertPkStringArgs::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertPkStringArgs { s, data } = __args;
        __callback(__identity, __addr, __status, s, data);
    })
}

#[allow(unused)]
pub fn once_on_insert_pk_string(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &String, &i32) + Send + 'static,
) -> ReducerCallbackId<InsertPkStringArgs> {
    InsertPkStringArgs::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertPkStringArgs { s, data } = __args;
        __callback(__identity, __addr, __status, s, data);
    })
}

#[allow(unused)]
pub fn remove_on_insert_pk_string(id: ReducerCallbackId<InsertPkStringArgs>) {
    InsertPkStringArgs::remove_on_reducer(id);
}
