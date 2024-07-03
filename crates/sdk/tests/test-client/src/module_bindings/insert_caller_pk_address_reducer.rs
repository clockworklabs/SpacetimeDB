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
pub struct InsertCallerPkAddressArgs {
    pub data: i32,
}

impl Reducer for InsertCallerPkAddressArgs {
    const REDUCER_NAME: &'static str = "insert_caller_pk_address";
}

#[allow(unused)]
pub fn insert_caller_pk_address(data: i32) {
    InsertCallerPkAddressArgs { data }.invoke();
}

#[allow(unused)]
pub fn on_insert_caller_pk_address(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &i32) + Send + 'static,
) -> ReducerCallbackId<InsertCallerPkAddressArgs> {
    InsertCallerPkAddressArgs::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertCallerPkAddressArgs { data } = __args;
        __callback(__identity, __addr, __status, data);
    })
}

#[allow(unused)]
pub fn once_on_insert_caller_pk_address(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &i32) + Send + 'static,
) -> ReducerCallbackId<InsertCallerPkAddressArgs> {
    InsertCallerPkAddressArgs::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertCallerPkAddressArgs { data } = __args;
        __callback(__identity, __addr, __status, data);
    })
}

#[allow(unused)]
pub fn remove_on_insert_caller_pk_address(id: ReducerCallbackId<InsertCallerPkAddressArgs>) {
    InsertCallerPkAddressArgs::remove_on_reducer(id);
}
