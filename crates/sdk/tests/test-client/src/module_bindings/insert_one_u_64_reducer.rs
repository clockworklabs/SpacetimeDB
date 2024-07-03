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
pub struct InsertOneU64Args {
    pub n: u64,
}

impl Reducer for InsertOneU64Args {
    const REDUCER_NAME: &'static str = "insert_one_u64";
}

#[allow(unused)]
pub fn insert_one_u_64(n: u64) {
    InsertOneU64Args { n }.invoke();
}

#[allow(unused)]
pub fn on_insert_one_u_64(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &u64) + Send + 'static,
) -> ReducerCallbackId<InsertOneU64Args> {
    InsertOneU64Args::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOneU64Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_insert_one_u_64(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &u64) + Send + 'static,
) -> ReducerCallbackId<InsertOneU64Args> {
    InsertOneU64Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOneU64Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_insert_one_u_64(id: ReducerCallbackId<InsertOneU64Args>) {
    InsertOneU64Args::remove_on_reducer(id);
}
