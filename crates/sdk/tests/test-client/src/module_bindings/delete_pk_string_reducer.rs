// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#[allow(unused)]
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
pub struct DeletePkStringArgs {
    pub s: String,
}

impl Reducer for DeletePkStringArgs {
    const REDUCER_NAME: &'static str = "delete_pk_string";
}

#[allow(unused)]
pub fn delete_pk_string(s: String) {
    DeletePkStringArgs { s }.invoke();
}

#[allow(unused)]
pub fn on_delete_pk_string(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &String) + Send + 'static,
) -> ReducerCallbackId<DeletePkStringArgs> {
    DeletePkStringArgs::on_reducer(move |__identity, __addr, __status, __args| {
        let DeletePkStringArgs { s } = __args;
        __callback(__identity, __addr, __status, s);
    })
}

#[allow(unused)]
pub fn once_on_delete_pk_string(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &String) + Send + 'static,
) -> ReducerCallbackId<DeletePkStringArgs> {
    DeletePkStringArgs::once_on_reducer(move |__identity, __addr, __status, __args| {
        let DeletePkStringArgs { s } = __args;
        __callback(__identity, __addr, __status, s);
    })
}

#[allow(unused)]
pub fn remove_on_delete_pk_string(id: ReducerCallbackId<DeletePkStringArgs>) {
    DeletePkStringArgs::remove_on_reducer(id);
}
