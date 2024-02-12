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
pub struct InsertUniqueI128Args {
    pub n: i128,
    pub data: i32,
}

impl Reducer for InsertUniqueI128Args {
    const REDUCER_NAME: &'static str = "insert_unique_i128";
}

#[allow(unused)]
pub fn insert_unique_i_128(n: i128, data: i32) {
    InsertUniqueI128Args { n, data }.invoke();
}

#[allow(unused)]
pub fn on_insert_unique_i_128(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &i128, &i32) + Send + 'static,
) -> ReducerCallbackId<InsertUniqueI128Args> {
    InsertUniqueI128Args::on_reducer(move |__identity, __addr, __status, __args| {
        let InsertUniqueI128Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn once_on_insert_unique_i_128(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &i128, &i32) + Send + 'static,
) -> ReducerCallbackId<InsertUniqueI128Args> {
    InsertUniqueI128Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let InsertUniqueI128Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn remove_on_insert_unique_i_128(id: ReducerCallbackId<InsertUniqueI128Args>) {
    InsertUniqueI128Args::remove_on_reducer(id);
}
