// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct InsertUniqueAddressArgs {
    pub a: __sdk::Address,
    pub data: i32,
}

impl From<InsertUniqueAddressArgs> for super::Reducer {
    fn from(args: InsertUniqueAddressArgs) -> Self {
        Self::InsertUniqueAddress {
            a: args.a,
            data: args.data,
        }
    }
}

impl __sdk::InModule for InsertUniqueAddressArgs {
    type Module = super::RemoteModule;
}

pub struct InsertUniqueAddressCallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `insert_unique_address`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait insert_unique_address {
    /// Request that the remote module invoke the reducer `insert_unique_address` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_insert_unique_address`] callbacks.
    fn insert_unique_address(&self, a: __sdk::Address, data: i32) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `insert_unique_address`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InsertUniqueAddressCallbackId`] can be passed to [`Self::remove_on_insert_unique_address`]
    /// to cancel the callback.
    fn on_insert_unique_address(
        &self,
        callback: impl FnMut(&super::EventContext, &__sdk::Address, &i32) + Send + 'static,
    ) -> InsertUniqueAddressCallbackId;
    /// Cancel a callback previously registered by [`Self::on_insert_unique_address`],
    /// causing it not to run in the future.
    fn remove_on_insert_unique_address(&self, callback: InsertUniqueAddressCallbackId);
}

impl insert_unique_address for super::RemoteReducers {
    fn insert_unique_address(&self, a: __sdk::Address, data: i32) -> __anyhow::Result<()> {
        self.imp
            .call_reducer("insert_unique_address", InsertUniqueAddressArgs { a, data })
    }
    fn on_insert_unique_address(
        &self,
        mut callback: impl FnMut(&super::EventContext, &__sdk::Address, &i32) + Send + 'static,
    ) -> InsertUniqueAddressCallbackId {
        InsertUniqueAddressCallbackId(self.imp.on_reducer(
            "insert_unique_address",
            Box::new(move |ctx: &super::EventContext| {
                let super::EventContext {
                    event:
                        __sdk::Event::Reducer(__sdk::ReducerEvent {
                            reducer: super::Reducer::InsertUniqueAddress { a, data },
                            ..
                        }),
                    ..
                } = ctx
                else {
                    unreachable!()
                };
                callback(ctx, a, data)
            }),
        ))
    }
    fn remove_on_insert_unique_address(&self, callback: InsertUniqueAddressCallbackId) {
        self.imp.remove_on_reducer("insert_unique_address", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `insert_unique_address`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_insert_unique_address {
    /// Set the call-reducer flags for the reducer `insert_unique_address` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn insert_unique_address(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_insert_unique_address for super::SetReducerFlags {
    fn insert_unique_address(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("insert_unique_address", flags);
    }
}
