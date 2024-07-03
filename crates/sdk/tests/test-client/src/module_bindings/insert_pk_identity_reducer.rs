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
pub struct InsertPkIdentityArgs {
    pub i: Identity,
    pub data: i32,
}

impl Reducer for InsertPkIdentityArgs {
    const REDUCER_NAME: &'static str = "insert_pk_identity";
}

#[allow(unused)]
pub fn insert_pk_identity(i: Identity, data: i32) {
    InsertPkIdentityArgs { i, data }.invoke();
}

#[allow(unused)]
pub fn on_insert_pk_identity(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &Identity, &i32) + Send + 'static,
) -> ReducerCallbackId<InsertPkIdentityArgs> {
    InsertPkIdentityArgs::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertPkIdentityArgs { i, data } = __args;
        __callback(__identity, __addr, __status, i, data);
    })
}

#[allow(unused)]
pub fn once_on_insert_pk_identity(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &Identity, &i32) + Send + 'static,
) -> ReducerCallbackId<InsertPkIdentityArgs> {
    InsertPkIdentityArgs::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertPkIdentityArgs { i, data } = __args;
        __callback(__identity, __addr, __status, i, data);
    })
}

#[allow(unused)]
pub fn remove_on_insert_pk_identity(id: ReducerCallbackId<InsertPkIdentityArgs>) {
    InsertPkIdentityArgs::remove_on_reducer(id);
}
