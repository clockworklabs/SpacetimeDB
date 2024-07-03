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
pub struct UpdatePkU16Args {
    pub n: u16,
    pub data: i32,
}

impl Reducer for UpdatePkU16Args {
    const REDUCER_NAME: &'static str = "update_pk_u16";
}

#[allow(unused)]
pub fn update_pk_u_16(n: u16, data: i32) {
    UpdatePkU16Args { n, data }.invoke();
}

#[allow(unused)]
pub fn on_update_pk_u_16(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &u16, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdatePkU16Args> {
    UpdatePkU16Args::on_reducer(move |__identity, __addr, __status, __args| {
        let UpdatePkU16Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn once_on_update_pk_u_16(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &u16, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdatePkU16Args> {
    UpdatePkU16Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let UpdatePkU16Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn remove_on_update_pk_u_16(id: ReducerCallbackId<UpdatePkU16Args>) {
    UpdatePkU16Args::remove_on_reducer(id);
}
