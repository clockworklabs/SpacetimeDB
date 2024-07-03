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
pub struct InsertUniqueU256Args {
    pub n: u256,
    pub data: i32,
}

impl Reducer for InsertUniqueU256Args {
    const REDUCER_NAME: &'static str = "insert_unique_u256";
}

#[allow(unused)]
pub fn insert_unique_u_256(n: u256, data: i32) {
    InsertUniqueU256Args { n, data }.invoke();
}

#[allow(unused)]
pub fn on_insert_unique_u_256(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &u256, &i32) + Send + 'static,
) -> ReducerCallbackId<InsertUniqueU256Args> {
    InsertUniqueU256Args::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertUniqueU256Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn once_on_insert_unique_u_256(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &u256, &i32) + Send + 'static,
) -> ReducerCallbackId<InsertUniqueU256Args> {
    InsertUniqueU256Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertUniqueU256Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn remove_on_insert_unique_u_256(id: ReducerCallbackId<InsertUniqueU256Args>) {
    InsertUniqueU256Args::remove_on_reducer(id);
}
