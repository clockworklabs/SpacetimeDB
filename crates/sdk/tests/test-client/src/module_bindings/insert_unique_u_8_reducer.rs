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
pub struct InsertUniqueU8 {
    pub n: u8,
    pub data: i32,
}

impl __sdk::spacetime_module::InModule for InsertUniqueU8 {
    type Module = super::RemoteModule;
}

pub struct InsertUniqueU8CallbackId(__sdk::callbacks::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `insert_unique_u8`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait insert_unique_u_8 {
    /// Request that the remote module invoke the reducer `insert_unique_u8` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_insert_unique_u_8`] callbacks.
    fn insert_unique_u_8(&self, n: u8, data: i32) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `insert_unique_u8`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InsertUniqueU8CallbackId`] can be passed to [`Self::remove_on_insert_unique_u_8`]
    /// to cancel the callback.
    fn on_insert_unique_u_8(
        &self,
        callback: impl FnMut(&super::EventContext, &u8, &i32) + Send + 'static,
    ) -> InsertUniqueU8CallbackId;
    /// Cancel a callback previously registered by [`Self::on_insert_unique_u_8`],
    /// causing it not to run in the future.
    fn remove_on_insert_unique_u_8(&self, callback: InsertUniqueU8CallbackId);
}

impl insert_unique_u_8 for super::RemoteReducers {
    fn insert_unique_u_8(&self, n: u8, data: i32) -> __anyhow::Result<()> {
        self.imp.call_reducer("insert_unique_u8", InsertUniqueU8 { n, data })
    }
    fn on_insert_unique_u_8(
        &self,
        mut callback: impl FnMut(&super::EventContext, &u8, &i32) + Send + 'static,
    ) -> InsertUniqueU8CallbackId {
        InsertUniqueU8CallbackId(self.imp.on_reducer::<InsertUniqueU8>(
            "insert_unique_u8",
            Box::new(move |ctx: &super::EventContext, args: &InsertUniqueU8| callback(ctx, &args.n, &args.data)),
        ))
    }
    fn remove_on_insert_unique_u_8(&self, callback: InsertUniqueU8CallbackId) {
        self.imp
            .remove_on_reducer::<InsertUniqueU8>("insert_unique_u8", callback.0)
    }
}
