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
pub struct InsertOptionIdentityArgs {
    pub i: Option<Identity>,
}

impl Reducer for InsertOptionIdentityArgs {
    const REDUCER_NAME: &'static str = "insert_option_identity";
}

#[allow(unused)]
pub fn insert_option_identity(i: Option<Identity>) {
    InsertOptionIdentityArgs { i }.invoke();
}

#[allow(unused)]
pub fn on_insert_option_identity(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &Option<Identity>) + Send + 'static,
) -> ReducerCallbackId<InsertOptionIdentityArgs> {
    InsertOptionIdentityArgs::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOptionIdentityArgs { i } = __args;
        __callback(__identity, __addr, __status, i);
    })
}

#[allow(unused)]
pub fn once_on_insert_option_identity(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &Option<Identity>) + Send + 'static,
) -> ReducerCallbackId<InsertOptionIdentityArgs> {
    InsertOptionIdentityArgs::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOptionIdentityArgs { i } = __args;
        __callback(__identity, __addr, __status, i);
    })
}

#[allow(unused)]
pub fn remove_on_insert_option_identity(id: ReducerCallbackId<InsertOptionIdentityArgs>) {
    InsertOptionIdentityArgs::remove_on_reducer(id);
}
