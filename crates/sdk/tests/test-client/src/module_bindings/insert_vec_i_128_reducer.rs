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
pub struct InsertVecI128Args {
    pub n: Vec<i128>,
}

impl Reducer for InsertVecI128Args {
    const REDUCER_NAME: &'static str = "insert_vec_i128";
}

#[allow(unused)]
pub fn insert_vec_i_128(n: Vec<i128>) {
    InsertVecI128Args { n }.invoke();
}

#[allow(unused)]
pub fn on_insert_vec_i_128(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &Vec<i128>) + Send + 'static,
) -> ReducerCallbackId<InsertVecI128Args> {
    InsertVecI128Args::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertVecI128Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_insert_vec_i_128(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &Vec<i128>) + Send + 'static,
) -> ReducerCallbackId<InsertVecI128Args> {
    InsertVecI128Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertVecI128Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_insert_vec_i_128(id: ReducerCallbackId<InsertVecI128Args>) {
    InsertVecI128Args::remove_on_reducer(id);
}
