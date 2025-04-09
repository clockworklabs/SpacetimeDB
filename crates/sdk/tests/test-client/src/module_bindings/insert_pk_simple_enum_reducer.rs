// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{self as __sdk, __lib, __sats, __ws};

use super::simple_enum_type::SimpleEnum;

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct InsertPkSimpleEnumArgs {
    pub a: SimpleEnum,
    pub data: i32,
}

impl From<InsertPkSimpleEnumArgs> for super::Reducer {
    fn from(args: InsertPkSimpleEnumArgs) -> Self {
        Self::InsertPkSimpleEnum {
            a: args.a,
            data: args.data,
        }
    }
}

impl __sdk::InModule for InsertPkSimpleEnumArgs {
    type Module = super::RemoteModule;
}

pub struct InsertPkSimpleEnumCallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `insert_pk_simple_enum`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait insert_pk_simple_enum {
    /// Request that the remote module invoke the reducer `insert_pk_simple_enum` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_insert_pk_simple_enum`] callbacks.
    fn insert_pk_simple_enum(&self, a: SimpleEnum, data: i32) -> __sdk::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `insert_pk_simple_enum`.
    ///
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::ReducerEventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InsertPkSimpleEnumCallbackId`] can be passed to [`Self::remove_on_insert_pk_simple_enum`]
    /// to cancel the callback.
    fn on_insert_pk_simple_enum(
        &self,
        callback: impl FnMut(&super::ReducerEventContext, &SimpleEnum, &i32) + Send + 'static,
    ) -> InsertPkSimpleEnumCallbackId;
    /// Cancel a callback previously registered by [`Self::on_insert_pk_simple_enum`],
    /// causing it not to run in the future.
    fn remove_on_insert_pk_simple_enum(&self, callback: InsertPkSimpleEnumCallbackId);
}

impl insert_pk_simple_enum for super::RemoteReducers {
    fn insert_pk_simple_enum(&self, a: SimpleEnum, data: i32) -> __sdk::Result<()> {
        self.imp
            .call_reducer("insert_pk_simple_enum", InsertPkSimpleEnumArgs { a, data })
    }
    fn on_insert_pk_simple_enum(
        &self,
        mut callback: impl FnMut(&super::ReducerEventContext, &SimpleEnum, &i32) + Send + 'static,
    ) -> InsertPkSimpleEnumCallbackId {
        InsertPkSimpleEnumCallbackId(self.imp.on_reducer(
            "insert_pk_simple_enum",
            Box::new(move |ctx: &super::ReducerEventContext| {
                let super::ReducerEventContext {
                    event:
                        __sdk::ReducerEvent {
                            reducer: super::Reducer::InsertPkSimpleEnum { a, data },
                            ..
                        },
                    ..
                } = ctx
                else {
                    unreachable!()
                };
                callback(ctx, a, data)
            }),
        ))
    }
    fn remove_on_insert_pk_simple_enum(&self, callback: InsertPkSimpleEnumCallbackId) {
        self.imp.remove_on_reducer("insert_pk_simple_enum", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `insert_pk_simple_enum`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_insert_pk_simple_enum {
    /// Set the call-reducer flags for the reducer `insert_pk_simple_enum` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn insert_pk_simple_enum(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_insert_pk_simple_enum for super::SetReducerFlags {
    fn insert_pk_simple_enum(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("insert_pk_simple_enum", flags);
    }
}
