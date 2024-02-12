// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

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
pub struct InsertOneI16Args {
    pub n: i16,
}

impl Reducer for InsertOneI16Args {
    const REDUCER_NAME: &'static str = "insert_one_i16";
}

#[allow(unused)]
pub fn insert_one_i_16(n: i16) {
    InsertOneI16Args { n }.invoke();
}

#[allow(unused)]
pub fn on_insert_one_i_16(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &i16) + Send + 'static,
) -> ReducerCallbackId<InsertOneI16Args> {
    InsertOneI16Args::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOneI16Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_insert_one_i_16(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &i16) + Send + 'static,
) -> ReducerCallbackId<InsertOneI16Args> {
    InsertOneI16Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertOneI16Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_insert_one_i_16(id: ReducerCallbackId<InsertOneI16Args>) {
    InsertOneI16Args::remove_on_reducer(id);
}
