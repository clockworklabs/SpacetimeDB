// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{self as __sdk, __lib, __sats, __ws};

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct UpdatePkBoolArgs {
    pub b: bool,
    pub data: i32,
}

impl From<UpdatePkBoolArgs> for super::Reducer {
    fn from(args: UpdatePkBoolArgs) -> Self {
        Self::UpdatePkBool {
            b: args.b,
            data: args.data,
        }
    }
}

impl __sdk::InModule for UpdatePkBoolArgs {
    type Module = super::RemoteModule;
}

pub struct UpdatePkBoolCallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `update_pk_bool`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait update_pk_bool {
    /// Request that the remote module invoke the reducer `update_pk_bool` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_update_pk_bool`] callbacks.
    fn update_pk_bool(&self, b: bool, data: i32) -> __sdk::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `update_pk_bool`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`UpdatePkBoolCallbackId`] can be passed to [`Self::remove_on_update_pk_bool`]
    /// to cancel the callback.
    fn on_update_pk_bool(
        &self,
        callback: impl FnMut(&super::EventContext, &bool, &i32) + Send + 'static,
    ) -> UpdatePkBoolCallbackId;
    /// Cancel a callback previously registered by [`Self::on_update_pk_bool`],
    /// causing it not to run in the future.
    fn remove_on_update_pk_bool(&self, callback: UpdatePkBoolCallbackId);
}

impl update_pk_bool for super::RemoteReducers {
    fn update_pk_bool(&self, b: bool, data: i32) -> __sdk::Result<()> {
        self.imp.call_reducer("update_pk_bool", UpdatePkBoolArgs { b, data })
    }
    fn on_update_pk_bool(
        &self,
        mut callback: impl FnMut(&super::EventContext, &bool, &i32) + Send + 'static,
    ) -> UpdatePkBoolCallbackId {
        UpdatePkBoolCallbackId(self.imp.on_reducer(
            "update_pk_bool",
            Box::new(move |ctx: &super::EventContext| {
                let super::EventContext {
                    event:
                        __sdk::Event::Reducer(__sdk::ReducerEvent {
                            reducer: super::Reducer::UpdatePkBool { b, data },
                            ..
                        }),
                    ..
                } = ctx
                else {
                    unreachable!()
                };
                callback(ctx, b, data)
            }),
        ))
    }
    fn remove_on_update_pk_bool(&self, callback: UpdatePkBoolCallbackId) {
        self.imp.remove_on_reducer("update_pk_bool", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `update_pk_bool`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_update_pk_bool {
    /// Set the call-reducer flags for the reducer `update_pk_bool` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn update_pk_bool(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_update_pk_bool for super::SetReducerFlags {
    fn update_pk_bool(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("update_pk_bool", flags);
    }
}
