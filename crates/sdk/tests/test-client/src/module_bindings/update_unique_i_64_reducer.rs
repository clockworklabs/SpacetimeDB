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
pub struct UpdateUniqueI64Args {
    pub n: i64,
    pub data: i32,
}

impl Reducer for UpdateUniqueI64Args {
    const REDUCER_NAME: &'static str = "update_unique_i64";
}

#[allow(unused)]
pub fn update_unique_i_64(n: i64, data: i32) {
    UpdateUniqueI64Args { n, data }.invoke();
}

#[allow(unused)]
pub fn on_update_unique_i_64(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &i64, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdateUniqueI64Args> {
    UpdateUniqueI64Args::on_reducer(move |__identity, __addr, __status, __args| {
        let UpdateUniqueI64Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn once_on_update_unique_i_64(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &i64, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdateUniqueI64Args> {
    UpdateUniqueI64Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let UpdateUniqueI64Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn remove_on_update_unique_i_64(id: ReducerCallbackId<UpdateUniqueI64Args>) {
    UpdateUniqueI64Args::remove_on_reducer(id);
}
