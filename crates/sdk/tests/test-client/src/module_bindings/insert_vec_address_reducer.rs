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
pub struct InsertVecAddress {
    pub a: Vec<__sdk::Address>,
}

impl __sdk::spacetime_module::InModule for InsertVecAddress {
    type Module = super::RemoteModule;
}

pub struct InsertVecAddressCallbackId(__sdk::callbacks::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `insert_vec_address`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait insert_vec_address {
    /// Request that the remote module invoke the reducer `insert_vec_address` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_insert_vec_address`] callbacks.
    fn insert_vec_address(&self, a: Vec<__sdk::Address>) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `insert_vec_address`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InsertVecAddressCallbackId`] can be passed to [`Self::remove_on_insert_vec_address`]
    /// to cancel the callback.
    fn on_insert_vec_address(
        &self,
        callback: impl FnMut(&super::EventContext, &Vec<__sdk::Address>) + Send + 'static,
    ) -> InsertVecAddressCallbackId;
    /// Cancel a callback previously registered by [`Self::on_insert_vec_address`],
    /// causing it not to run in the future.
    fn remove_on_insert_vec_address(&self, callback: InsertVecAddressCallbackId);
}

impl insert_vec_address for super::RemoteReducers {
    fn insert_vec_address(&self, a: Vec<__sdk::Address>) -> __anyhow::Result<()> {
        self.imp.call_reducer("insert_vec_address", InsertVecAddress { a })
    }
    fn on_insert_vec_address(
        &self,
        mut callback: impl FnMut(&super::EventContext, &Vec<__sdk::Address>) + Send + 'static,
    ) -> InsertVecAddressCallbackId {
        InsertVecAddressCallbackId(self.imp.on_reducer::<InsertVecAddress>(
            "insert_vec_address",
            Box::new(move |ctx: &super::EventContext, args: &InsertVecAddress| callback(ctx, &args.a)),
        ))
    }
    fn remove_on_insert_vec_address(&self, callback: InsertVecAddressCallbackId) {
        self.imp
            .remove_on_reducer::<InsertVecAddress>("insert_vec_address", callback.0)
    }
}
