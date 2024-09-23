// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub struct Init {}

impl __sdk::spacetime_module::InModule for Init {
    type Module = super::RemoteModule;
}

pub struct InitCallbackId(__sdk::callbacks::CallbackId);

#[allow(non_camel_case_types)]
pub trait init {
    fn init(&self) -> __anyhow::Result<()>;
    fn on_init(&self, callback: impl FnMut(&super::EventContext) + Send + 'static) -> InitCallbackId;
    fn remove_on_init(&self, callback: InitCallbackId);
}

impl init for super::RemoteReducers {
    fn init(&self) -> __anyhow::Result<()> {
        self.imp.call_reducer("__init__", Init {})
    }
    fn on_init(&self, mut callback: impl FnMut(&super::EventContext) + Send + 'static) -> InitCallbackId {
        InitCallbackId(self.imp.on_reducer::<Init>(
            "__init__",
            Box::new(move |ctx: &super::EventContext, args: &Init| callback(ctx)),
        ))
    }
    fn remove_on_init(&self, callback: InitCallbackId) {
        self.imp.remove_on_reducer::<Init>("__init__", callback.0)
    }
}