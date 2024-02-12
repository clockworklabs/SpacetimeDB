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
pub struct UpdatePkU8Args {
    pub n: u8,
    pub data: i32,
}

impl Reducer for UpdatePkU8Args {
    const REDUCER_NAME: &'static str = "update_pk_u8";
}

#[allow(unused)]
pub fn update_pk_u_8(n: u8, data: i32) {
    UpdatePkU8Args { n, data }.invoke();
}

#[allow(unused)]
pub fn on_update_pk_u_8(
    mut __callback: impl FnMut(&Identity, Option<Address>, &Status, &u8, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdatePkU8Args> {
    UpdatePkU8Args::on_reducer(move |__identity, __addr, __status, __args| {
        let UpdatePkU8Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn once_on_update_pk_u_8(
    __callback: impl FnOnce(&Identity, Option<Address>, &Status, &u8, &i32) + Send + 'static,
) -> ReducerCallbackId<UpdatePkU8Args> {
    UpdatePkU8Args::once_on_reducer(move |__identity, __addr, __status, __args| {
        let UpdatePkU8Args { n, data } = __args;
        __callback(__identity, __addr, __status, n, data);
    })
}

#[allow(unused)]
pub fn remove_on_update_pk_u_8(id: ReducerCallbackId<UpdatePkU8Args>) {
    UpdatePkU8Args::remove_on_reducer(id);
}
