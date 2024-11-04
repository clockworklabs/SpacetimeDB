// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub struct DeletePkAddress {
    pub a: __sdk::Address,
}

impl __sdk::InModule for DeletePkAddress {
    type Module = super::RemoteModule;
}

pub struct DeletePkAddressCallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `delete_pk_address`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait delete_pk_address {
    /// Request that the remote module invoke the reducer `delete_pk_address` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_delete_pk_address`] callbacks.
    fn delete_pk_address(&self, a: __sdk::Address) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `delete_pk_address`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`DeletePkAddressCallbackId`] can be passed to [`Self::remove_on_delete_pk_address`]
    /// to cancel the callback.
    fn on_delete_pk_address(
        &self,
        callback: impl FnMut(&super::EventContext, &__sdk::Address) + Send + 'static,
    ) -> DeletePkAddressCallbackId;
    /// Cancel a callback previously registered by [`Self::on_delete_pk_address`],
    /// causing it not to run in the future.
    fn remove_on_delete_pk_address(&self, callback: DeletePkAddressCallbackId);
}

impl delete_pk_address for super::RemoteReducers {
    fn delete_pk_address(&self, a: __sdk::Address) -> __anyhow::Result<()> {
        self.imp.call_reducer("delete_pk_address", DeletePkAddress { a })
    }
    fn on_delete_pk_address(
        &self,
        mut callback: impl FnMut(&super::EventContext, &__sdk::Address) + Send + 'static,
    ) -> DeletePkAddressCallbackId {
        DeletePkAddressCallbackId(self.imp.on_reducer::<DeletePkAddress>(
            "delete_pk_address",
            Box::new(move |ctx: &super::EventContext, args: &DeletePkAddress| callback(ctx, &args.a)),
        ))
    }
    fn remove_on_delete_pk_address(&self, callback: DeletePkAddressCallbackId) {
        self.imp
            .remove_on_reducer::<DeletePkAddress>("delete_pk_address", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `delete_pk_address`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_delete_pk_address {
    /// Set the call-reducer flags for the reducer `delete_pk_address` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn delete_pk_address(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_delete_pk_address for super::SetReducerFlags {
    fn delete_pk_address(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("delete_pk_address", flags);
    }
}
