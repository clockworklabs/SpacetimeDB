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
pub struct UpdatePkU32Args {
    pub n: u32,
    pub data: i32,
}

impl Reducer for UpdatePkU32Args {
    const REDUCER_NAME: &'static str = "update_pk_u32";
}

#[allow(unused)]
pub fn update_pk_u_32(n: u32, data: i32) {
    UpdatePkU32Args { n, data }.invoke();
}

#[allow(unused)]
pub fn on_update_pk_u_32(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &u32, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdatePkU32Args> {
    UpdatePkU32Args::on_reducer(move |__identity, __addr, __status, __args| {
        let UpdatePkU32Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn once_on_update_pk_u_32(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &u32, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdatePkU32Args> {
    UpdatePkU32Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let UpdatePkU32Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn remove_on_update_pk_u_32(id: ReducerCallbackId<UpdatePkU32Args>) {
    UpdatePkU32Args::remove_on_reducer(id);
}
