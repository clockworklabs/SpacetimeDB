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
pub struct DeletePkI8Args {
    pub n: i8,
}

impl Reducer for DeletePkI8Args {
    const REDUCER_NAME: &'static str = "delete_pk_i8";
}

#[allow(unused)]
pub fn delete_pk_i_8(n: i8) {
    DeletePkI8Args { n }.invoke();
}

#[allow(unused)]
pub fn on_delete_pk_i_8(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &i8) + Send + 'static,
) -> ReducerCallbackId<DeletePkI8Args> {
    DeletePkI8Args::on_reducer(move |__identity, __addr, __status, __args| {
        let DeletePkI8Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_delete_pk_i_8(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &i8) + Send + 'static,
) -> ReducerCallbackId<DeletePkI8Args> {
    DeletePkI8Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let DeletePkI8Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_delete_pk_i_8(id: ReducerCallbackId<DeletePkI8Args>) {
    DeletePkI8Args::remove_on_reducer(id);
}
