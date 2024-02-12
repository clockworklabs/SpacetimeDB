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
pub struct DeleteUniqueI64Args {
    pub n: i64,
}

impl Reducer for DeleteUniqueI64Args {
    const REDUCER_NAME: &'static str = "delete_unique_i64";
}

#[allow(unused)]
pub fn delete_unique_i_64(n: i64) {
    DeleteUniqueI64Args { n }.invoke();
}

#[allow(unused)]
pub fn on_delete_unique_i_64(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &i64) + Send + 'static,
) -> ReducerCallbackId<DeleteUniqueI64Args> {
    DeleteUniqueI64Args::on_reducer(move |__identity, __addr, __status, __args| {
        let DeleteUniqueI64Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn once_on_delete_unique_i_64(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &i64) + Send + 'static,
) -> ReducerCallbackId<DeleteUniqueI64Args> {
    DeleteUniqueI64Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let DeleteUniqueI64Args { n } = __args;
        __callback(__identity, __addr, __status, n);
    })
}

#[allow(unused)]
pub fn remove_on_delete_unique_i_64(id: ReducerCallbackId<DeleteUniqueI64Args>) {
    DeleteUniqueI64Args::remove_on_reducer(id);
}
