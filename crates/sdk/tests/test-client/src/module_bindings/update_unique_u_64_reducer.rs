// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct UpdateUniqueU64Args {
    pub n: u64,
    pub data: i32,
}

impl From<UpdateUniqueU64Args> for super::Reducer {
    fn from(args: UpdateUniqueU64Args) -> Self {
        Self::UpdateUniqueU64 {
            n: args.n,
            data: args.data,
        }
    }
}

impl __sdk::InModule for UpdateUniqueU64Args {
    type Module = super::RemoteModule;
}

pub struct UpdateUniqueU64CallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `update_unique_u64`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait update_unique_u_64 {
    /// Request that the remote module invoke the reducer `update_unique_u64` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_update_unique_u_64`] callbacks.
    fn update_unique_u_64(&self, n: u64, data: i32) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `update_unique_u64`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`UpdateUniqueU64CallbackId`] can be passed to [`Self::remove_on_update_unique_u_64`]
    /// to cancel the callback.
    fn on_update_unique_u_64(
        &self,
        callback: impl FnMut(&super::EventContext, &u64, &i32) + Send + 'static,
    ) -> UpdateUniqueU64CallbackId;
    /// Cancel a callback previously registered by [`Self::on_update_unique_u_64`],
    /// causing it not to run in the future.
    fn remove_on_update_unique_u_64(&self, callback: UpdateUniqueU64CallbackId);
}

impl update_unique_u_64 for super::RemoteReducers {
    fn update_unique_u_64(&self, n: u64, data: i32) -> __anyhow::Result<()> {
        self.imp
            .call_reducer("update_unique_u64", UpdateUniqueU64Args { n, data })
    }
    fn on_update_unique_u_64(
        &self,
        mut callback: impl FnMut(&super::EventContext, &u64, &i32) + Send + 'static,
    ) -> UpdateUniqueU64CallbackId {
        UpdateUniqueU64CallbackId(self.imp.on_reducer(
            "update_unique_u64",
            Box::new(move |ctx: &super::EventContext| {
                let super::EventContext {
                    event:
                        __sdk::Event::Reducer(__sdk::ReducerEvent {
                            reducer: super::Reducer::UpdateUniqueU64 { n, data },
                            ..
                        }),
                    ..
                } = ctx
                else {
                    unreachable!()
                };
                callback(ctx, n, data)
            }),
        ))
    }
    fn remove_on_update_unique_u_64(&self, callback: UpdateUniqueU64CallbackId) {
        self.imp.remove_on_reducer("update_unique_u64", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `update_unique_u64`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_update_unique_u_64 {
    /// Set the call-reducer flags for the reducer `update_unique_u64` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn update_unique_u_64(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_update_unique_u_64 for super::SetReducerFlags {
    fn update_unique_u_64(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("update_unique_u64", flags);
    }
}
