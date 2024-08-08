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
pub struct InsertOptionVecOptionI32Args {
    pub v: Option<Vec<Option<i32>>>,
}

impl Reducer for InsertOptionVecOptionI32Args {
    const REDUCER_NAME: &'static str = "insert_option_vec_option_i32";
}

#[allow(unused)]
pub fn insert_option_vec_option_i_32(v: Option<Vec<Option<i32>>>) {
    InsertOptionVecOptionI32Args { v }.invoke();
}

#[allow(unused)]
pub fn on_insert_option_vec_option_i_32(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &Option<Vec<Option<i32>>>) + Send + 'static,
) -> ReducerCallbackId<InsertOptionVecOptionI32Args> {
    InsertOptionVecOptionI32Args::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOptionVecOptionI32Args { v } = __args;
        __callback(__identity, __addr, __status, v);
    })
}

#[allow(unused)]
pub fn once_on_insert_option_vec_option_i_32(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &Option<Vec<Option<i32>>>) + Send + 'static,
) -> ReducerCallbackId<InsertOptionVecOptionI32Args> {
    InsertOptionVecOptionI32Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOptionVecOptionI32Args { v } = __args;
        __callback(__identity, __addr, __status, v);
    })
}

#[allow(unused)]
pub fn remove_on_insert_option_vec_option_i_32(id: ReducerCallbackId<InsertOptionVecOptionI32Args>) {
    InsertOptionVecOptionI32Args::remove_on_reducer(id);
}
