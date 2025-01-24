// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{self as __sdk, __lib, __sats, __ws};

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct DeletePkStringArgs {
    pub s: String,
}

impl From<DeletePkStringArgs> for super::Reducer {
    fn from(args: DeletePkStringArgs) -> Self {
        Self::DeletePkString { s: args.s }
    }
}

impl __sdk::InModule for DeletePkStringArgs {
    type Module = super::RemoteModule;
}

pub struct DeletePkStringCallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `delete_pk_string`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait delete_pk_string {
    /// Request that the remote module invoke the reducer `delete_pk_string` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_delete_pk_string`] callbacks.
    fn delete_pk_string(&self, s: String) -> __sdk::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `delete_pk_string`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`DeletePkStringCallbackId`] can be passed to [`Self::remove_on_delete_pk_string`]
    /// to cancel the callback.
    fn on_delete_pk_string(
        &self,
        callback: impl FnMut(&super::EventContext, &String) + Send + 'static,
    ) -> DeletePkStringCallbackId;
    /// Cancel a callback previously registered by [`Self::on_delete_pk_string`],
    /// causing it not to run in the future.
    fn remove_on_delete_pk_string(&self, callback: DeletePkStringCallbackId);
}

impl delete_pk_string for super::RemoteReducers {
    fn delete_pk_string(&self, s: String) -> __sdk::Result<()> {
        self.imp.call_reducer("delete_pk_string", DeletePkStringArgs { s })
    }
    fn on_delete_pk_string(
        &self,
        mut callback: impl FnMut(&super::EventContext, &String) + Send + 'static,
    ) -> DeletePkStringCallbackId {
        DeletePkStringCallbackId(self.imp.on_reducer(
            "delete_pk_string",
            Box::new(move |ctx: &super::EventContext| {
                let super::EventContext {
                    event:
                        __sdk::Event::Reducer(__sdk::ReducerEvent {
                            reducer: super::Reducer::DeletePkString { s },
                            ..
                        }),
                    ..
                } = ctx
                else {
                    unreachable!()
                };
                callback(ctx, s)
            }),
        ))
    }
    fn remove_on_delete_pk_string(&self, callback: DeletePkStringCallbackId) {
        self.imp.remove_on_reducer("delete_pk_string", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `delete_pk_string`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_delete_pk_string {
    /// Set the call-reducer flags for the reducer `delete_pk_string` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn delete_pk_string(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_delete_pk_string for super::SetReducerFlags {
    fn delete_pk_string(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("delete_pk_string", flags);
    }
}
