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
pub struct InsertOneF64 {
    pub f: f64,
}

impl __sdk::spacetime_module::InModule for InsertOneF64 {
    type Module = super::RemoteModule;
}

pub struct InsertOneF64CallbackId(__sdk::callbacks::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `insert_one_f64`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait insert_one_f_64 {
    /// Request that the remote module invoke the reducer `insert_one_f64` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_insert_one_f_64`] callbacks.
    fn insert_one_f_64(&self, f: f64) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `insert_one_f64`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InsertOneF64CallbackId`] can be passed to [`Self::remove_on_insert_one_f_64`]
    /// to cancel the callback.
    fn on_insert_one_f_64(
        &self,
        callback: impl FnMut(&super::EventContext, &f64) + Send + 'static,
    ) -> InsertOneF64CallbackId;
    /// Cancel a callback previously registered by [`Self::on_insert_one_f_64`],
    /// causing it not to run in the future.
    fn remove_on_insert_one_f_64(&self, callback: InsertOneF64CallbackId);
}

impl insert_one_f_64 for super::RemoteReducers {
    fn insert_one_f_64(&self, f: f64) -> __anyhow::Result<()> {
        self.imp.call_reducer(48, InsertOneF64 { f })
    }
    fn on_insert_one_f_64(
        &self,
        mut callback: impl FnMut(&super::EventContext, &f64) + Send + 'static,
    ) -> InsertOneF64CallbackId {
        InsertOneF64CallbackId(self.imp.on_reducer::<InsertOneF64>(
            48,
            Box::new(move |ctx: &super::EventContext, args: &InsertOneF64| callback(ctx, &args.f)),
        ))
    }
    fn remove_on_insert_one_f_64(&self, callback: InsertOneF64CallbackId) {
        self.imp.remove_on_reducer::<InsertOneF64>(48, callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `insert_one_f64`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_insert_one_f_64 {
    /// Set the call-reducer flags for the reducer `insert_one_f64` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn insert_one_f_64(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_insert_one_f_64 for super::SetReducerFlags {
    fn insert_one_f_64(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags(48, flags);
    }
}
