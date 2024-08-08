// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused_imports)]
use super::every_vec_struct::EveryVecStruct;
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
pub struct InsertOneEveryVecStructArgs {
    pub s: EveryVecStruct,
}

impl Reducer for InsertOneEveryVecStructArgs {
    const REDUCER_NAME: &'static str = "insert_one_every_vec_struct";
}

#[allow(unused)]
pub fn insert_one_every_vec_struct(s: EveryVecStruct) {
    InsertOneEveryVecStructArgs { s }.invoke();
}

#[allow(unused)]
pub fn on_insert_one_every_vec_struct(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &EveryVecStruct) + Send + 'static,
) -> ReducerCallbackId<InsertOneEveryVecStructArgs> {
    InsertOneEveryVecStructArgs::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOneEveryVecStructArgs { s } = __args;
        __callback(__identity, __addr, __status, s);
    })
}

#[allow(unused)]
pub fn once_on_insert_one_every_vec_struct(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &EveryVecStruct) + Send + 'static,
) -> ReducerCallbackId<InsertOneEveryVecStructArgs> {
    InsertOneEveryVecStructArgs::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOneEveryVecStructArgs { s } = __args;
        __callback(__identity, __addr, __status, s);
    })
}

#[allow(unused)]
pub fn remove_on_insert_one_every_vec_struct(id: ReducerCallbackId<InsertOneEveryVecStructArgs>) {
    InsertOneEveryVecStructArgs::remove_on_reducer(id);
}
