// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused_imports)]
use spacetimedb_sdk::{
    anyhow::{anyhow, Result},
    identity::Identity,
    reducer::{Reducer, ReducerCallbackId, Status},
    sats::{de::Deserialize, i256, ser::Serialize, u256},
    spacetimedb_lib,
    table::{TableIter, TableType, TableWithPrimaryKey},
    Address,
};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct PkAddress {
    pub a: Address,
    pub data: i32,
}

impl TableType for PkAddress {
    const TABLE_NAME: &'static str = "PkAddress";
    type ReducerEvent = super::ReducerEvent;
}

impl TableWithPrimaryKey for PkAddress {
    type PrimaryKey = Address;
    fn primary_key(&self) -> &Self::PrimaryKey {
        &self.a
    }
}

impl PkAddress {
    #[allow(unused)]
    pub fn filter_by_a(a: Address) -> TableIter<Self> {
        Self::filter(|row| row.a == a)
    }
    #[allow(unused)]
    pub fn find_by_a(a: Address) -> Option<Self> {
        Self::find(|row| row.a == a)
    }
    #[allow(unused)]
    pub fn filter_by_data(data: i32) -> TableIter<Self> {
        Self::filter(|row| row.data == data)
    }
}
