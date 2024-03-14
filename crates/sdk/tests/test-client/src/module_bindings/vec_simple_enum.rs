// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused_imports)]
use super::simple_enum::SimpleEnum;
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
pub struct VecSimpleEnum {
    pub e: Vec<SimpleEnum>,
}

impl TableType for VecSimpleEnum {
    const TABLE_NAME: &'static str = "VecSimpleEnum";
    type ReducerEvent = super::ReducerEvent;
}

impl VecSimpleEnum {
    #[allow(unused)]
    pub fn filter_by_e(e: Vec<SimpleEnum>) -> TableIter<Self> {
        Self::filter(|row| row.e == e)
    }
}
