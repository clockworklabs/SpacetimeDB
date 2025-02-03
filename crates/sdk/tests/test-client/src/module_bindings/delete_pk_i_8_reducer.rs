// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct DeletePkI8Args {
    pub n: i8,
}

impl From<DeletePkI8Args> for super::Reducer {
    fn from(args: DeletePkI8Args) -> Self {
        Self::DeletePkI8 { n: args.n }
    }
}

impl __sdk::InModule for DeletePkI8Args {
    type Module = super::RemoteModule;
}

pub struct DeletePkI8CallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `delete_pk_i8`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait delete_pk_i_8 {
    /// Request that the remote module invoke the reducer `delete_pk_i8` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_delete_pk_i_8`] callbacks.
    fn delete_pk_i_8(&self, n: i8) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `delete_pk_i8`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`DeletePkI8CallbackId`] can be passed to [`Self::remove_on_delete_pk_i_8`]
    /// to cancel the callback.
    fn on_delete_pk_i_8(
        &self,
        callback: impl FnMut(&super::EventContext, &i8) + Send + 'static,
    ) -> DeletePkI8CallbackId;
    /// Cancel a callback previously registered by [`Self::on_delete_pk_i_8`],
    /// causing it not to run in the future.
    fn remove_on_delete_pk_i_8(&self, callback: DeletePkI8CallbackId);
}

impl delete_pk_i_8 for super::RemoteReducers {
    fn delete_pk_i_8(&self, n: i8) -> __anyhow::Result<()> {
        self.imp.call_reducer("delete_pk_i8", DeletePkI8Args { n })
    }
    fn on_delete_pk_i_8(
        &self,
        mut callback: impl FnMut(&super::EventContext, &i8) + Send + 'static,
    ) -> DeletePkI8CallbackId {
        DeletePkI8CallbackId(self.imp.on_reducer(
            "delete_pk_i8",
            Box::new(move |ctx: &super::EventContext| {
                let super::EventContext {
                    event:
                        __sdk::Event::Reducer(__sdk::ReducerEvent {
                            reducer: super::Reducer::DeletePkI8 { n },
                            ..
                        }),
                    ..
                } = ctx
                else {
                    unreachable!()
                };
                callback(ctx, n)
            }),
        ))
    }
    fn remove_on_delete_pk_i_8(&self, callback: DeletePkI8CallbackId) {
        self.imp.remove_on_reducer("delete_pk_i8", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `delete_pk_i8`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_delete_pk_i_8 {
    /// Set the call-reducer flags for the reducer `delete_pk_i8` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn delete_pk_i_8(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_delete_pk_i_8 for super::SetReducerFlags {
    fn delete_pk_i_8(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("delete_pk_i8", flags);
    }
}
