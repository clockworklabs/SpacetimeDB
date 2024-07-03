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
pub struct DeleteUniqueU256Args {
    pub n: u256,
}

impl Reducer for DeleteUniqueU256Args {
    const REDUCER_NAME: &'static str = "delete_unique_u256";
}

#[allow(unused)]
pub fn delete_unique_u_256(n: u256) {
    DeleteUniqueU256Args { n }.invoke();
}

#[allow(unused)]
pub fn on_delete_unique_u_256(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &u256) + Send + 'static,
) -> ReducerCallbackId<DeleteUniqueU256Args> {
    DeleteUniqueU256Args::on_reducer(move |__identity, __addr, __status, __args| {
        let DeleteUniqueU256Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_delete_unique_u_256(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &u256) + Send + 'static,
) -> ReducerCallbackId<DeleteUniqueU256Args> {
    DeleteUniqueU256Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let DeleteUniqueU256Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_delete_unique_u_256(id: ReducerCallbackId<DeleteUniqueU256Args>) {
    DeleteUniqueU256Args::remove_on_reducer(id);
}
