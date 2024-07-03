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
pub struct InsertOneU32Args {
    pub n: u32,
}

impl Reducer for InsertOneU32Args {
    const REDUCER_NAME: &'static str = "insert_one_u32";
}

#[allow(unused)]
pub fn insert_one_u_32(n: u32) {
    InsertOneU32Args { n }.invoke();
}

#[allow(unused)]
pub fn on_insert_one_u_32(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &u32) + Send + 'static,
) -> ReducerCallbackId<InsertOneU32Args> {
    InsertOneU32Args::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOneU32Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_insert_one_u_32(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &u32) + Send + 'static,
) -> ReducerCallbackId<InsertOneU32Args> {
    InsertOneU32Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOneU32Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_insert_one_u_32(id: ReducerCallbackId<InsertOneU32Args>) {
    InsertOneU32Args::remove_on_reducer(id);
}
