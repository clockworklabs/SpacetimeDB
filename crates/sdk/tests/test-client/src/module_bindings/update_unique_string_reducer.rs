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
pub struct UpdateUniqueString {
    pub s: String,
    pub data: i32,
}

impl __sdk::spacetime_module::InModule for UpdateUniqueString {
    type Module = super::RemoteModule;
}

pub struct UpdateUniqueStringCallbackId(__sdk::callbacks::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `update_unique_string`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait update_unique_string {
    /// Request that the remote module invoke the reducer `update_unique_string` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_update_unique_string`] callbacks.
    fn update_unique_string(&self, s: String, data: i32) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `update_unique_string`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`UpdateUniqueStringCallbackId`] can be passed to [`Self::remove_on_update_unique_string`]
    /// to cancel the callback.
    fn on_update_unique_string(
        &self,
        callback: impl FnMut(&super::EventContext, &String, &i32) + Send + 'static,
    ) -> UpdateUniqueStringCallbackId;
    /// Cancel a callback previously registered by [`Self::on_update_unique_string`],
    /// causing it not to run in the future.
    fn remove_on_update_unique_string(&self, callback: UpdateUniqueStringCallbackId);
}

impl update_unique_string for super::RemoteReducers {
    fn update_unique_string(&self, s: String, data: i32) -> __anyhow::Result<()> {
        self.imp
            .call_reducer("update_unique_string", UpdateUniqueString { s, data })
    }
    fn on_update_unique_string(
        &self,
        mut callback: impl FnMut(&super::EventContext, &String, &i32) + Send + 'static,
    ) -> UpdateUniqueStringCallbackId {
        UpdateUniqueStringCallbackId(self.imp.on_reducer::<UpdateUniqueString>(
            "update_unique_string",
            Box::new(move |ctx: &super::EventContext, args: &UpdateUniqueString| callback(ctx, &args.s, &args.data)),
        ))
    }
    fn remove_on_update_unique_string(&self, callback: UpdateUniqueStringCallbackId) {
        self.imp
            .remove_on_reducer::<UpdateUniqueString>("update_unique_string", callback.0)
    }
}
