// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{self as __sdk, __lib, __sats, __ws};

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct UpdateUniqueIdentityArgs {
    pub i: __sdk::Identity,
    pub data: i32,
}

impl From<UpdateUniqueIdentityArgs> for super::Reducer {
    fn from(args: UpdateUniqueIdentityArgs) -> Self {
        Self::UpdateUniqueIdentity {
            i: args.i,
            data: args.data,
        }
    }
}

impl __sdk::InModule for UpdateUniqueIdentityArgs {
    type Module = super::RemoteModule;
}

pub struct UpdateUniqueIdentityCallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `update_unique_identity`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait update_unique_identity {
    /// Request that the remote module invoke the reducer `update_unique_identity` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_update_unique_identity`] callbacks.
    fn update_unique_identity(&self, i: __sdk::Identity, data: i32) -> __sdk::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `update_unique_identity`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`UpdateUniqueIdentityCallbackId`] can be passed to [`Self::remove_on_update_unique_identity`]
    /// to cancel the callback.
    fn on_update_unique_identity(
        &self,
        callback: impl FnMut(&super::EventContext, &__sdk::Identity, &i32) + Send + 'static,
    ) -> UpdateUniqueIdentityCallbackId;
    /// Cancel a callback previously registered by [`Self::on_update_unique_identity`],
    /// causing it not to run in the future.
    fn remove_on_update_unique_identity(&self, callback: UpdateUniqueIdentityCallbackId);
}

impl update_unique_identity for super::RemoteReducers {
    fn update_unique_identity(&self, i: __sdk::Identity, data: i32) -> __sdk::Result<()> {
        self.imp
            .call_reducer("update_unique_identity", UpdateUniqueIdentityArgs { i, data })
    }
    fn on_update_unique_identity(
        &self,
        mut callback: impl FnMut(&super::EventContext, &__sdk::Identity, &i32) + Send + 'static,
    ) -> UpdateUniqueIdentityCallbackId {
        UpdateUniqueIdentityCallbackId(self.imp.on_reducer(
            "update_unique_identity",
            Box::new(move |ctx: &super::EventContext| {
                let super::EventContext {
                    event:
                        __sdk::Event::Reducer(__sdk::ReducerEvent {
                            reducer: super::Reducer::UpdateUniqueIdentity { i, data },
                            ..
                        }),
                    ..
                } = ctx
                else {
                    unreachable!()
                };
                callback(ctx, i, data)
            }),
        ))
    }
    fn remove_on_update_unique_identity(&self, callback: UpdateUniqueIdentityCallbackId) {
        self.imp.remove_on_reducer("update_unique_identity", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `update_unique_identity`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_update_unique_identity {
    /// Set the call-reducer flags for the reducer `update_unique_identity` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn update_unique_identity(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_update_unique_identity for super::SetReducerFlags {
    fn update_unique_identity(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("update_unique_identity", flags);
    }
}
