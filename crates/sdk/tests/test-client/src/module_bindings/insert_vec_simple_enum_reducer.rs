// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{self as __sdk, __lib, __sats, __ws};

use super::simple_enum_type::SimpleEnum;

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct InsertVecSimpleEnumArgs {
    pub e: Vec<SimpleEnum>,
}

impl From<InsertVecSimpleEnumArgs> for super::Reducer {
    fn from(args: InsertVecSimpleEnumArgs) -> Self {
        Self::InsertVecSimpleEnum { e: args.e }
    }
}

impl __sdk::InModule for InsertVecSimpleEnumArgs {
    type Module = super::RemoteModule;
}

pub struct InsertVecSimpleEnumCallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `insert_vec_simple_enum`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait insert_vec_simple_enum {
    /// Request that the remote module invoke the reducer `insert_vec_simple_enum` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_insert_vec_simple_enum`] callbacks.
    fn insert_vec_simple_enum(&self, e: Vec<SimpleEnum>) -> __sdk::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `insert_vec_simple_enum`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InsertVecSimpleEnumCallbackId`] can be passed to [`Self::remove_on_insert_vec_simple_enum`]
    /// to cancel the callback.
    fn on_insert_vec_simple_enum(
        &self,
        callback: impl FnMut(&super::EventContext, &Vec<SimpleEnum>) + Send + 'static,
    ) -> InsertVecSimpleEnumCallbackId;
    /// Cancel a callback previously registered by [`Self::on_insert_vec_simple_enum`],
    /// causing it not to run in the future.
    fn remove_on_insert_vec_simple_enum(&self, callback: InsertVecSimpleEnumCallbackId);
}

impl insert_vec_simple_enum for super::RemoteReducers {
    fn insert_vec_simple_enum(&self, e: Vec<SimpleEnum>) -> __sdk::Result<()> {
        self.imp
            .call_reducer("insert_vec_simple_enum", InsertVecSimpleEnumArgs { e })
    }
    fn on_insert_vec_simple_enum(
        &self,
        mut callback: impl FnMut(&super::EventContext, &Vec<SimpleEnum>) + Send + 'static,
    ) -> InsertVecSimpleEnumCallbackId {
        InsertVecSimpleEnumCallbackId(self.imp.on_reducer(
            "insert_vec_simple_enum",
            Box::new(move |ctx: &super::EventContext| {
                let super::EventContext {
                    event:
                        __sdk::Event::Reducer(__sdk::ReducerEvent {
                            reducer: super::Reducer::InsertVecSimpleEnum { e },
                            ..
                        }),
                    ..
                } = ctx
                else {
                    unreachable!()
                };
                callback(ctx, e)
            }),
        ))
    }
    fn remove_on_insert_vec_simple_enum(&self, callback: InsertVecSimpleEnumCallbackId) {
        self.imp.remove_on_reducer("insert_vec_simple_enum", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `insert_vec_simple_enum`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_insert_vec_simple_enum {
    /// Set the call-reducer flags for the reducer `insert_vec_simple_enum` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn insert_vec_simple_enum(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_insert_vec_simple_enum for super::SetReducerFlags {
    fn insert_vec_simple_enum(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("insert_vec_simple_enum", flags);
    }
}
