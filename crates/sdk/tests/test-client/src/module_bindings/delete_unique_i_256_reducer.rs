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
pub struct DeleteUniqueI256Args {
    pub n: i256,
}

impl Reducer for DeleteUniqueI256Args {
    const REDUCER_NAME: &'static str = "delete_unique_i256";
}

#[allow(unused)]
pub fn delete_unique_i_256(n: i256) {
    DeleteUniqueI256Args { n }.invoke();
}

#[allow(unused)]
pub fn on_delete_unique_i_256(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &i256) + Send + 'static,
) -> ReducerCallbackId<DeleteUniqueI256Args> {
    DeleteUniqueI256Args::on_reducer(move |__identity, __addr, __status, __args| {
        let DeleteUniqueI256Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_delete_unique_i_256(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &i256) + Send + 'static,
) -> ReducerCallbackId<DeleteUniqueI256Args> {
    DeleteUniqueI256Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let DeleteUniqueI256Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_delete_unique_i_256(id: ReducerCallbackId<DeleteUniqueI256Args>) {
    DeleteUniqueI256Args::remove_on_reducer(id);
}
