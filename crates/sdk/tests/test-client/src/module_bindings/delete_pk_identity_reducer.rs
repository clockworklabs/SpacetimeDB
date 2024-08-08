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
pub struct DeletePkIdentityArgs {
    pub i: Identity,
}

impl Reducer for DeletePkIdentityArgs {
    const REDUCER_NAME: &'static str = "delete_pk_identity";
}

#[allow(unused)]
pub fn delete_pk_identity(i: Identity) {
    DeletePkIdentityArgs { i }.invoke();
}

#[allow(unused)]
pub fn on_delete_pk_identity(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &Identity) + Send + 'static,
) -> ReducerCallbackId<DeletePkIdentityArgs> {
    DeletePkIdentityArgs::on_reducer(move |__identity, __addr, __status, __args| {
        let DeletePkIdentityArgs { i } = __args;
        __callback(__identity, __addr, __status, i);
    })
}

#[allow(unused)]
pub fn once_on_delete_pk_identity(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &Identity) + Send + 'static,
) -> ReducerCallbackId<DeletePkIdentityArgs> {
    DeletePkIdentityArgs::once_on_reducer(move |__identity, __addr, __status, __args| {
        let DeletePkIdentityArgs { i } = __args;
        __callback(__identity, __addr, __status, i);
    })
}

#[allow(unused)]
pub fn remove_on_delete_pk_identity(id: ReducerCallbackId<DeletePkIdentityArgs>) {
    DeletePkIdentityArgs::remove_on_reducer(id);
}
