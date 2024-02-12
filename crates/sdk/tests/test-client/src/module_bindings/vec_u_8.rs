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
pub struct VecU8 {
    pub n: Vec<u8>,
}

impl TableType for VecU8 {
    const TABLE_NAME: &'static str = "VecU8";
    type ReducerEvent = super::ReducerEvent;
}

impl VecU8 {
    #[allow(unused)]
    pub fn filter_by_n(n: Vec<u8>) -> TableIter<Self> {
        Self::filter(|row| row.n == n)
    }
}
