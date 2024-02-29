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
pub struct InsertVecU64Args {
    pub n: Vec<u64>,
}

impl Reducer for InsertVecU64Args {
    const REDUCER_NAME: &'static str = "insert_vec_u64";
}

#[allow(unused)]
pub fn insert_vec_u_64(n: Vec<u64>) {
    InsertVecU64Args { n }.invoke();
}

#[allow(unused)]
pub fn on_insert_vec_u_64(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &Vec<u64>) + Send + 'static,
) -> ReducerCallbackId<InsertVecU64Args> {
    InsertVecU64Args::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertVecU64Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_insert_vec_u_64(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &Vec<u64>) + Send + 'static,
) -> ReducerCallbackId<InsertVecU64Args> {
    InsertVecU64Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertVecU64Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_insert_vec_u_64(id: ReducerCallbackId<InsertVecU64Args>) {
    InsertVecU64Args::remove_on_reducer(id);
}
