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
pub struct InsertVecBool {
    pub b: Vec<bool>,
}

impl __sdk::spacetime_module::InModule for InsertVecBool {
    type Module = super::RemoteModule;
}

pub struct InsertVecBoolCallbackId(__sdk::callbacks::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `insert_vec_bool`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait insert_vec_bool {
    /// Request that the remote module invoke the reducer `insert_vec_bool` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_insert_vec_bool`] callbacks.
    fn insert_vec_bool(&self, b: Vec<bool>) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `insert_vec_bool`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InsertVecBoolCallbackId`] can be passed to [`Self::remove_on_insert_vec_bool`]
    /// to cancel the callback.
    fn on_insert_vec_bool(
        &self,
        callback: impl FnMut(&super::EventContext, &Vec<bool>) + Send + 'static,
    ) -> InsertVecBoolCallbackId;
    /// Cancel a callback previously registered by [`Self::on_insert_vec_bool`],
    /// causing it not to run in the future.
    fn remove_on_insert_vec_bool(&self, callback: InsertVecBoolCallbackId);
}

impl insert_vec_bool for super::RemoteReducers {
    fn insert_vec_bool(&self, b: Vec<bool>) -> __anyhow::Result<()> {
        self.imp.call_reducer("insert_vec_bool", InsertVecBool { b })
    }
    fn on_insert_vec_bool(
        &self,
        mut callback: impl FnMut(&super::EventContext, &Vec<bool>) + Send + 'static,
    ) -> InsertVecBoolCallbackId {
        InsertVecBoolCallbackId(self.imp.on_reducer::<InsertVecBool>(
            "insert_vec_bool",
            Box::new(move |ctx: &super::EventContext, args: &InsertVecBool| callback(ctx, &args.b)),
        ))
    }
    fn remove_on_insert_vec_bool(&self, callback: InsertVecBoolCallbackId) {
        self.imp
            .remove_on_reducer::<InsertVecBool>("insert_vec_bool", callback.0)
    }
}
