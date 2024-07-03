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
pub struct UpdateUniqueU64Args {
    pub n: u64,
    pub data: i32,
}

impl Reducer for UpdateUniqueU64Args {
    const REDUCER_NAME: &'static str = "update_unique_u64";
}

#[allow(unused)]
pub fn update_unique_u_64(n: u64, data: i32) {
    UpdateUniqueU64Args { n, data }.invoke();
}

#[allow(unused)]
pub fn on_update_unique_u_64(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &u64, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdateUniqueU64Args> {
    UpdateUniqueU64Args::on_reducer(move |__identity, __addr, __status, __args| {
        let UpdateUniqueU64Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn once_on_update_unique_u_64(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &u64, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdateUniqueU64Args> {
    UpdateUniqueU64Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let UpdateUniqueU64Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn remove_on_update_unique_u_64(id: ReducerCallbackId<UpdateUniqueU64Args>) {
    UpdateUniqueU64Args::remove_on_reducer(id);
}
