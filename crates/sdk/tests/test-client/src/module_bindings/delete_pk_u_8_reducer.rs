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
pub struct DeletePkU8Args {
    pub n: u8,
}

impl Reducer for DeletePkU8Args {
    const REDUCER_NAME: &'static str = "delete_pk_u8";
}

#[allow(unused)]
pub fn delete_pk_u_8(n: u8) {
    DeletePkU8Args { n }.invoke();
}

#[allow(unused)]
pub fn on_delete_pk_u_8(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &u8) + Send + 'static,
) -> ReducerCallbackId<DeletePkU8Args> {
    DeletePkU8Args::on_reducer(move |__identity, __addr, __status, __args| {
        let DeletePkU8Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_delete_pk_u_8(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &u8) + Send + 'static,
) -> ReducerCallbackId<DeletePkU8Args> {
    DeletePkU8Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let DeletePkU8Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_delete_pk_u_8(id: ReducerCallbackId<DeletePkU8Args>) {
    DeletePkU8Args::remove_on_reducer(id);
}
