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
pub struct VecString {
    pub s: Vec<String>,
}

impl TableType for VecString {
    const TABLE_NAME: &'static str = "VecString";
    type ReducerEvent = super::ReducerEvent;
}

impl VecString {
    #[allow(unused)]
    pub fn filter_by_s(s: Vec<String>) -> TableIter<Self> {
        Self::filter(|row| row.s == s)
    }
}
