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
pub struct DeleteUniqueI128Args {
    pub n: i128,
}

impl Reducer for DeleteUniqueI128Args {
    const REDUCER_NAME: &'static str = "delete_unique_i128";
}

#[allow(unused)]
pub fn delete_unique_i_128(n: i128) {
    DeleteUniqueI128Args { n }.invoke();
}

#[allow(unused)]
pub fn on_delete_unique_i_128(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &i128) + Send + 'static,
) -> ReducerCallbackId<DeleteUniqueI128Args> {
    DeleteUniqueI128Args::on_reducer(move |__identity, __addr, __status, __args| {
        let DeleteUniqueI128Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_delete_unique_i_128(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &i128) + Send + 'static,
) -> ReducerCallbackId<DeleteUniqueI128Args> {
    DeleteUniqueI128Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let DeleteUniqueI128Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_delete_unique_i_128(id: ReducerCallbackId<DeleteUniqueI128Args>) {
    DeleteUniqueI128Args::remove_on_reducer(id);
}
