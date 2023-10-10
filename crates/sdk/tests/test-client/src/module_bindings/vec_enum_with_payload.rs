// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

use super::enum_with_payload::EnumWithPayload;
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
pub struct VecEnumWithPayload {
    pub e: Vec<EnumWithPayload>,
}

impl TableType for VecEnumWithPayload {
    const TABLE_NAME: &'static str = "VecEnumWithPayload";
    type ReducerEvent = super::ReducerEvent;
}

impl VecEnumWithPayload {
    #[allow(unused)]
    pub fn filter_by_e(e: Vec<EnumWithPayload>) -> TableIter<Self> {
        Self::filter(|row| row.e == e)
    }
}
