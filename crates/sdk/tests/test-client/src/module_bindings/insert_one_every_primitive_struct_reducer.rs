// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused_imports)]
use super::every_primitive_struct::EveryPrimitiveStruct;
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
pub struct InsertOneEveryPrimitiveStructArgs {
    pub s: EveryPrimitiveStruct,
}

impl Reducer for InsertOneEveryPrimitiveStructArgs {
    const REDUCER_NAME: &'static str = "insert_one_every_primitive_struct";
}

#[allow(unused)]
pub fn insert_one_every_primitive_struct(s: EveryPrimitiveStruct) {
    InsertOneEveryPrimitiveStructArgs { s }.invoke();
}

#[allow(unused)]
pub fn on_insert_one_every_primitive_struct(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &EveryPrimitiveStruct) + Send + 'static,
) -> ReducerCallbackId<InsertOneEveryPrimitiveStructArgs> {
    InsertOneEveryPrimitiveStructArgs::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOneEveryPrimitiveStructArgs { s } = __args;
        __callback(__identity, __addr, __status, s);
    })
}

#[allow(unused)]
pub fn once_on_insert_one_every_primitive_struct(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &EveryPrimitiveStruct) + Send + 'static,
) -> ReducerCallbackId<InsertOneEveryPrimitiveStructArgs> {
    InsertOneEveryPrimitiveStructArgs::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOneEveryPrimitiveStructArgs { s } = __args;
        __callback(__identity, __addr, __status, s);
    })
}

#[allow(unused)]
pub fn remove_on_insert_one_every_primitive_struct(id: ReducerCallbackId<InsertOneEveryPrimitiveStructArgs>) {
    InsertOneEveryPrimitiveStructArgs::remove_on_reducer(id);
}
