// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub struct InsertUniqueBool {
    pub b: bool,
    pub data: i32,
}

impl __sdk::InModule for InsertUniqueBool {
    type Module = super::RemoteModule;
}

pub struct InsertUniqueBoolCallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `insert_unique_bool`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait insert_unique_bool {
    /// Request that the remote module invoke the reducer `insert_unique_bool` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_insert_unique_bool`] callbacks.
    fn insert_unique_bool(&self, b: bool, data: i32) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `insert_unique_bool`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InsertUniqueBoolCallbackId`] can be passed to [`Self::remove_on_insert_unique_bool`]
    /// to cancel the callback.
    fn on_insert_unique_bool(
        &self,
        callback: impl FnMut(&super::EventContext, &bool, &i32) + Send + 'static,
    ) -> InsertUniqueBoolCallbackId;
    /// Cancel a callback previously registered by [`Self::on_insert_unique_bool`],
    /// causing it not to run in the future.
    fn remove_on_insert_unique_bool(&self, callback: InsertUniqueBoolCallbackId);
}

impl insert_unique_bool for super::RemoteReducers {
    fn insert_unique_bool(&self, b: bool, data: i32) -> __anyhow::Result<()> {
        self.imp
            .call_reducer("insert_unique_bool", InsertUniqueBool { b, data })
    }
    fn on_insert_unique_bool(
        &self,
        mut callback: impl FnMut(&super::EventContext, &bool, &i32) + Send + 'static,
    ) -> InsertUniqueBoolCallbackId {
        InsertUniqueBoolCallbackId(self.imp.on_reducer::<InsertUniqueBool>(
            "insert_unique_bool",
            Box::new(move |ctx: &super::EventContext, args: &InsertUniqueBool| callback(ctx, &args.b, &args.data)),
        ))
    }
    fn remove_on_insert_unique_bool(&self, callback: InsertUniqueBoolCallbackId) {
        self.imp
            .remove_on_reducer::<InsertUniqueBool>("insert_unique_bool", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `insert_unique_bool`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_insert_unique_bool {
    /// Set the call-reducer flags for the reducer `insert_unique_bool` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn insert_unique_bool(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_insert_unique_bool for super::SetReducerFlags {
    fn insert_unique_bool(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("insert_unique_bool", flags);
    }
}
