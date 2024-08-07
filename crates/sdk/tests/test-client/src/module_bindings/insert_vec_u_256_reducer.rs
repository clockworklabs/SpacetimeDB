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
pub struct InsertVecU256Args {
    pub n: Vec<u256>,
}

impl Reducer for InsertVecU256Args {
    const REDUCER_NAME: &'static str = "insert_vec_u256";
}

#[allow(unused)]
pub fn insert_vec_u_256(n: Vec<u256>) {
    InsertVecU256Args { n }.invoke();
}

#[allow(unused)]
pub fn on_insert_vec_u_256(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &Vec<u256>) + Send + 'static,
) -> ReducerCallbackId<InsertVecU256Args> {
    InsertVecU256Args::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertVecU256Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_insert_vec_u_256(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &Vec<u256>) + Send + 'static,
) -> ReducerCallbackId<InsertVecU256Args> {
    InsertVecU256Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertVecU256Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_insert_vec_u_256(id: ReducerCallbackId<InsertVecU256Args>) {
    InsertVecU256Args::remove_on_reducer(id);
}
