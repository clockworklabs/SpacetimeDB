// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused_imports)]
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
pub struct EveryPrimitiveStruct {
    pub a: u8,
    pub b: u16,
    pub c: u32,
    pub d: u64,
    pub e: u128,
    pub f: i8,
    pub g: i16,
    pub h: i32,
    pub i: i64,
    pub j: i128,
    pub k: bool,
    pub l: f32,
    pub m: f64,
    pub n: String,
    pub o: Identity,
    pub p: Address,
}
