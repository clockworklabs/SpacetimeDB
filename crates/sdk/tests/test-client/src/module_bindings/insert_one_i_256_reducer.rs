// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct InsertOneI256Args {
    pub n: __sats::i256,
}

impl From<InsertOneI256Args> for super::Reducer {
    fn from(args: InsertOneI256Args) -> Self {
        Self::InsertOneI256 { n: args.n }
    }
}

impl __sdk::InModule for InsertOneI256Args {
    type Module = super::RemoteModule;
}

pub struct InsertOneI256CallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `insert_one_i256`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait insert_one_i_256 {
    /// Request that the remote module invoke the reducer `insert_one_i256` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_insert_one_i_256`] callbacks.
    fn insert_one_i_256(&self, n: __sats::i256) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `insert_one_i256`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InsertOneI256CallbackId`] can be passed to [`Self::remove_on_insert_one_i_256`]
    /// to cancel the callback.
    fn on_insert_one_i_256(
        &self,
        callback: impl FnMut(&super::EventContext, &__sats::i256) + Send + 'static,
    ) -> InsertOneI256CallbackId;
    /// Cancel a callback previously registered by [`Self::on_insert_one_i_256`],
    /// causing it not to run in the future.
    fn remove_on_insert_one_i_256(&self, callback: InsertOneI256CallbackId);
}

impl insert_one_i_256 for super::RemoteReducers {
    fn insert_one_i_256(&self, n: __sats::i256) -> __anyhow::Result<()> {
        self.imp.call_reducer("insert_one_i256", InsertOneI256Args { n })
    }
    fn on_insert_one_i_256(
        &self,
        mut callback: impl FnMut(&super::EventContext, &__sats::i256) + Send + 'static,
    ) -> InsertOneI256CallbackId {
        InsertOneI256CallbackId(self.imp.on_reducer(
            "insert_one_i256",
            Box::new(move |ctx: &super::EventContext| {
                let super::EventContext {
                    event:
                        __sdk::Event::Reducer(__sdk::ReducerEvent {
                            reducer: super::Reducer::InsertOneI256 { n },
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
    fn remove_on_insert_one_i_256(&self, callback: InsertOneI256CallbackId) {
        self.imp.remove_on_reducer("insert_one_i256", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `insert_one_i256`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_insert_one_i_256 {
    /// Set the call-reducer flags for the reducer `insert_one_i256` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn insert_one_i_256(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_insert_one_i_256 for super::SetReducerFlags {
    fn insert_one_i_256(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("insert_one_i256", flags);
    }
}
