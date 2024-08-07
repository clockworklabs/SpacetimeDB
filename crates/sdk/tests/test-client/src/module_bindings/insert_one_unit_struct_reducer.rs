// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused_imports)]
use super::unit_struct::UnitStruct;
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
pub struct InsertOneUnitStructArgs {
    pub s: UnitStruct,
}

impl Reducer for InsertOneUnitStructArgs {
    const REDUCER_NAME: &'static str = "insert_one_unit_struct";
}

#[allow(unused)]
pub fn insert_one_unit_struct(s: UnitStruct) {
    InsertOneUnitStructArgs { s }.invoke();
}

#[allow(unused)]
pub fn on_insert_one_unit_struct(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &UnitStruct) + Send + 'static,
) -> ReducerCallbackId<InsertOneUnitStructArgs> {
    InsertOneUnitStructArgs::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOneUnitStructArgs { s } = __args;
        __callback(__identity, __addr, __status, s);
    })
}

#[allow(unused)]
pub fn once_on_insert_one_unit_struct(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &UnitStruct) + Send + 'static,
) -> ReducerCallbackId<InsertOneUnitStructArgs> {
    InsertOneUnitStructArgs::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOneUnitStructArgs { s } = __args;
        __callback(__identity, __addr, __status, s);
    })
}

#[allow(unused)]
pub fn remove_on_insert_one_unit_struct(id: ReducerCallbackId<InsertOneUnitStructArgs>) {
    InsertOneUnitStructArgs::remove_on_reducer(id);
}
