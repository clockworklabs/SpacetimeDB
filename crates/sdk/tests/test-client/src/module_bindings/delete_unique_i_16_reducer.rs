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
pub struct DeleteUniqueI16 {
    pub n: i16,
}

impl __sdk::spacetime_module::InModule for DeleteUniqueI16 {
    type Module = super::RemoteModule;
}

pub struct DeleteUniqueI16CallbackId(__sdk::callbacks::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `delete_unique_i16`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait delete_unique_i_16 {
    /// Request that the remote module invoke the reducer `delete_unique_i16` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_delete_unique_i_16`] callbacks.
    fn delete_unique_i_16(&self, n: i16) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `delete_unique_i16`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`DeleteUniqueI16CallbackId`] can be passed to [`Self::remove_on_delete_unique_i_16`]
    /// to cancel the callback.
    fn on_delete_unique_i_16(
        &self,
        callback: impl FnMut(&super::EventContext, &i16) + Send + 'static,
    ) -> DeleteUniqueI16CallbackId;
    /// Cancel a callback previously registered by [`Self::on_delete_unique_i_16`],
    /// causing it not to run in the future.
    fn remove_on_delete_unique_i_16(&self, callback: DeleteUniqueI16CallbackId);
}

impl delete_unique_i_16 for super::RemoteReducers {
    fn delete_unique_i_16(&self, n: i16) -> __anyhow::Result<()> {
        self.imp.call_reducer(19, DeleteUniqueI16 { n })
    }
    fn on_delete_unique_i_16(
        &self,
        mut callback: impl FnMut(&super::EventContext, &i16) + Send + 'static,
    ) -> DeleteUniqueI16CallbackId {
        DeleteUniqueI16CallbackId(self.imp.on_reducer::<DeleteUniqueI16>(
            19,
            Box::new(move |ctx: &super::EventContext, args: &DeleteUniqueI16| callback(ctx, &args.n)),
        ))
    }
    fn remove_on_delete_unique_i_16(&self, callback: DeleteUniqueI16CallbackId) {
        self.imp.remove_on_reducer::<DeleteUniqueI16>(19, callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `delete_unique_i16`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_delete_unique_i_16 {
    /// Set the call-reducer flags for the reducer `delete_unique_i16` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn delete_unique_i_16(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_delete_unique_i_16 for super::SetReducerFlags {
    fn delete_unique_i_16(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags(19, flags);
    }
}
