// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{self as __sdk, __lib, __sats, __ws};

use super::every_primitive_struct_type::EveryPrimitiveStruct;

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub struct OptionEveryPrimitiveStruct {
    pub s: Option<EveryPrimitiveStruct>,
}

impl __sdk::InModule for OptionEveryPrimitiveStruct {
    type Module = super::RemoteModule;
}
