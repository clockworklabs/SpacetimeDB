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
pub struct UpdatePkAddress {
    pub a: __sdk::Address,
    pub data: i32,
}

impl __sdk::spacetime_module::InModule for UpdatePkAddress {
    type Module = super::RemoteModule;
}

pub struct UpdatePkAddressCallbackId(__sdk::callbacks::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `update_pk_address`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait update_pk_address {
    /// Request that the remote module invoke the reducer `update_pk_address` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_update_pk_address`] callbacks.
    fn update_pk_address(&self, a: __sdk::Address, data: i32) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `update_pk_address`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`UpdatePkAddressCallbackId`] can be passed to [`Self::remove_on_update_pk_address`]
    /// to cancel the callback.
    fn on_update_pk_address(
        &self,
        callback: impl FnMut(&super::EventContext, &__sdk::Address, &i32) + Send + 'static,
    ) -> UpdatePkAddressCallbackId;
    /// Cancel a callback previously registered by [`Self::on_update_pk_address`],
    /// causing it not to run in the future.
    fn remove_on_update_pk_address(&self, callback: UpdatePkAddressCallbackId);
}

impl update_pk_address for super::RemoteReducers {
    fn update_pk_address(&self, a: __sdk::Address, data: i32) -> __anyhow::Result<()> {
        self.imp.call_reducer(130, UpdatePkAddress { a, data })
    }
    fn on_update_pk_address(
        &self,
        mut callback: impl FnMut(&super::EventContext, &__sdk::Address, &i32) + Send + 'static,
    ) -> UpdatePkAddressCallbackId {
        UpdatePkAddressCallbackId(self.imp.on_reducer::<UpdatePkAddress>(
            130,
            Box::new(move |ctx: &super::EventContext, args: &UpdatePkAddress| callback(ctx, &args.a, &args.data)),
        ))
    }
    fn remove_on_update_pk_address(&self, callback: UpdatePkAddressCallbackId) {
        self.imp.remove_on_reducer::<UpdatePkAddress>(130, callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `update_pk_address`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_update_pk_address {
    /// Set the call-reducer flags for the reducer `update_pk_address` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn update_pk_address(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_update_pk_address for super::SetReducerFlags {
    fn update_pk_address(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags(130, flags);
    }
}
