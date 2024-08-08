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
pub struct UpdateUniqueI8Args {
    pub n: i8,
    pub data: i32,
}

impl Reducer for UpdateUniqueI8Args {
    const REDUCER_NAME: &'static str = "update_unique_i8";
}

#[allow(unused)]
pub fn update_unique_i_8(n: i8, data: i32) {
    UpdateUniqueI8Args { n, data }.invoke();
}

#[allow(unused)]
pub fn on_update_unique_i_8(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &i8, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdateUniqueI8Args> {
    UpdateUniqueI8Args::on_reducer(move |__identity, __addr, __status, __args| {
        let UpdateUniqueI8Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn once_on_update_unique_i_8(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &i8, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdateUniqueI8Args> {
    UpdateUniqueI8Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let UpdateUniqueI8Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn remove_on_update_unique_i_8(id: ReducerCallbackId<UpdateUniqueI8Args>) {
    UpdateUniqueI8Args::remove_on_reducer(id);
}
