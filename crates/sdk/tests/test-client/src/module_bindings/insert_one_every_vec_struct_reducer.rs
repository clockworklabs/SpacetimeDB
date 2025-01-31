// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

use super::every_vec_struct_type::EveryVecStruct;

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct InsertOneEveryVecStructArgs {
    pub s: EveryVecStruct,
}

impl From<InsertOneEveryVecStructArgs> for super::Reducer {
    fn from(args: InsertOneEveryVecStructArgs) -> Self {
        Self::InsertOneEveryVecStruct { s: args.s }
    }
}

impl __sdk::InModule for InsertOneEveryVecStructArgs {
    type Module = super::RemoteModule;
}

pub struct InsertOneEveryVecStructCallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `insert_one_every_vec_struct`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait insert_one_every_vec_struct {
    /// Request that the remote module invoke the reducer `insert_one_every_vec_struct` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_insert_one_every_vec_struct`] callbacks.
    fn insert_one_every_vec_struct(&self, s: EveryVecStruct) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `insert_one_every_vec_struct`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InsertOneEveryVecStructCallbackId`] can be passed to [`Self::remove_on_insert_one_every_vec_struct`]
    /// to cancel the callback.
    fn on_insert_one_every_vec_struct(
        &self,
        callback: impl FnMut(&super::EventContext, &EveryVecStruct) + Send + 'static,
    ) -> InsertOneEveryVecStructCallbackId;
    /// Cancel a callback previously registered by [`Self::on_insert_one_every_vec_struct`],
    /// causing it not to run in the future.
    fn remove_on_insert_one_every_vec_struct(&self, callback: InsertOneEveryVecStructCallbackId);
}

impl insert_one_every_vec_struct for super::RemoteReducers {
    fn insert_one_every_vec_struct(&self, s: EveryVecStruct) -> __anyhow::Result<()> {
        self.imp
            .call_reducer("insert_one_every_vec_struct", InsertOneEveryVecStructArgs { s })
    }
    fn on_insert_one_every_vec_struct(
        &self,
        mut callback: impl FnMut(&super::EventContext, &EveryVecStruct) + Send + 'static,
    ) -> InsertOneEveryVecStructCallbackId {
        InsertOneEveryVecStructCallbackId(self.imp.on_reducer(
            "insert_one_every_vec_struct",
            Box::new(move |ctx: &super::EventContext| {
                let super::EventContext {
                    event:
                        __sdk::Event::Reducer(__sdk::ReducerEvent {
                            reducer: super::Reducer::InsertOneEveryVecStruct { s },
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
    fn remove_on_insert_one_every_vec_struct(&self, callback: InsertOneEveryVecStructCallbackId) {
        self.imp.remove_on_reducer("insert_one_every_vec_struct", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `insert_one_every_vec_struct`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_insert_one_every_vec_struct {
    /// Set the call-reducer flags for the reducer `insert_one_every_vec_struct` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn insert_one_every_vec_struct(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_insert_one_every_vec_struct for super::SetReducerFlags {
    fn insert_one_every_vec_struct(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("insert_one_every_vec_struct", flags);
    }
}
