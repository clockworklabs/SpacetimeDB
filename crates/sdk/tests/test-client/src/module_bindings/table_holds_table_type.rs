// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

use super::one_u_8_type::OneU8;
use super::vec_u_8_type::VecU8;

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub struct TableHoldsTable {
    pub a: OneU8,
    pub b: VecU8,
}

impl __sdk::spacetime_module::InModule for TableHoldsTable {
    type Module = super::RemoteModule;
}
