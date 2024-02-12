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
pub struct PkI32 {
    pub n: i32,
    pub data: i32,
}

impl TableType for PkI32 {
    const TABLE_NAME: &'static str = "PkI32";
    type ReducerEvent = super::ReducerEvent;
}

impl TableWithPrimaryKey for PkI32 {
    type PrimaryKey = i32;
    fn primary_key(&self) -> &Self::PrimaryKey {
        &self.n
    }
}

impl PkI32 {
    #[allow(unused)]
    pub fn filter_by_n(n: i32) -> Option<Self> {
        Self::find(|row| row.n == n)
    }
    #[allow(unused)]
    pub fn filter_by_data(data: i32) -> TableIter<Self> {
        Self::filter(|row| row.data == data)
    }
}
