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
pub struct DeletePkI128Args {
    pub n: i128,
}

impl Reducer for DeletePkI128Args {
    const REDUCER_NAME: &'static str = "delete_pk_i128";
}

#[allow(unused)]
pub fn delete_pk_i_128(n: i128) {
    DeletePkI128Args { n }.invoke();
}

#[allow(unused)]
pub fn on_delete_pk_i_128(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &i128) + Send + 'static,
) -> ReducerCallbackId<DeletePkI128Args> {
    DeletePkI128Args::on_reducer(move |__identity, __addr, __status, __args| {
        let DeletePkI128Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_delete_pk_i_128(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &i128) + Send + 'static,
) -> ReducerCallbackId<DeletePkI128Args> {
    DeletePkI128Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let DeletePkI128Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_delete_pk_i_128(id: ReducerCallbackId<DeletePkI128Args>) {
    DeletePkI128Args::remove_on_reducer(id);
}
