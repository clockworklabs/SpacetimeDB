// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused_imports)]
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
pub struct UpdatePkI32Args {
    pub n: i32,
    pub data: i32,
}

impl Reducer for UpdatePkI32Args {
    const REDUCER_NAME: &'static str = "update_pk_i32";
}

#[allow(unused)]
pub fn update_pk_i_32(n: i32, data: i32) {
    UpdatePkI32Args { n, data }.invoke();
}

#[allow(unused)]
pub fn on_update_pk_i_32(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &i32, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdatePkI32Args> {
    UpdatePkI32Args::on_reducer(move |__identity, __addr, __status, __args| {
        let UpdatePkI32Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn once_on_update_pk_i_32(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &i32, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdatePkI32Args> {
    UpdatePkI32Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let UpdatePkI32Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn remove_on_update_pk_i_32(id: ReducerCallbackId<UpdatePkI32Args>) {
    UpdatePkI32Args::remove_on_reducer(id);
}
