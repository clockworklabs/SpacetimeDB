// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused_imports)]
use super::every_vec_struct::EveryVecStruct;
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
pub struct VecEveryVecStruct {
    pub s: Vec<EveryVecStruct>,
}

impl TableType for VecEveryVecStruct {
    const TABLE_NAME: &'static str = "VecEveryVecStruct";
    type ReducerEvent = super::ReducerEvent;
}

impl VecEveryVecStruct {
    #[allow(unused)]
    pub fn filter_by_s(s: Vec<EveryVecStruct>) -> TableIter<Self> {
        Self::filter(|row| row.s == s)
    }
}
