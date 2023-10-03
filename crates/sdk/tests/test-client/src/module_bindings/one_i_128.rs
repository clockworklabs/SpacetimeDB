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
pub struct OneI128 {
    pub n: i128,
}

impl TableType for OneI128 {
    const TABLE_NAME: &'static str = "OneI128";
    type ReducerEvent = super::ReducerEvent;
}

impl OneI128 {
    #[allow(unused)]
    pub fn filter_by_n(n: i128) -> TableIter<Self> {
        Self::filter(|row| row.n == n)
    }
}
