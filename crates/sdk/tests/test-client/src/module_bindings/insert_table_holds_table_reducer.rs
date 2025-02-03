// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

use super::one_u_8_type::OneU8;
use super::vec_u_8_type::VecU8;

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct InsertTableHoldsTableArgs {
    pub a: OneU8,
    pub b: VecU8,
}

impl From<InsertTableHoldsTableArgs> for super::Reducer {
    fn from(args: InsertTableHoldsTableArgs) -> Self {
        Self::InsertTableHoldsTable { a: args.a, b: args.b }
    }
}

impl __sdk::InModule for InsertTableHoldsTableArgs {
    type Module = super::RemoteModule;
}

pub struct InsertTableHoldsTableCallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `insert_table_holds_table`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait insert_table_holds_table {
    /// Request that the remote module invoke the reducer `insert_table_holds_table` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_insert_table_holds_table`] callbacks.
    fn insert_table_holds_table(&self, a: OneU8, b: VecU8) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `insert_table_holds_table`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InsertTableHoldsTableCallbackId`] can be passed to [`Self::remove_on_insert_table_holds_table`]
    /// to cancel the callback.
    fn on_insert_table_holds_table(
        &self,
        callback: impl FnMut(&super::EventContext, &OneU8, &VecU8) + Send + 'static,
    ) -> InsertTableHoldsTableCallbackId;
    /// Cancel a callback previously registered by [`Self::on_insert_table_holds_table`],
    /// causing it not to run in the future.
    fn remove_on_insert_table_holds_table(&self, callback: InsertTableHoldsTableCallbackId);
}

impl insert_table_holds_table for super::RemoteReducers {
    fn insert_table_holds_table(&self, a: OneU8, b: VecU8) -> __anyhow::Result<()> {
        self.imp
            .call_reducer("insert_table_holds_table", InsertTableHoldsTableArgs { a, b })
    }
    fn on_insert_table_holds_table(
        &self,
        mut callback: impl FnMut(&super::EventContext, &OneU8, &VecU8) + Send + 'static,
    ) -> InsertTableHoldsTableCallbackId {
        InsertTableHoldsTableCallbackId(self.imp.on_reducer(
            "insert_table_holds_table",
            Box::new(move |ctx: &super::EventContext| {
                let super::EventContext {
                    event:
                        __sdk::Event::Reducer(__sdk::ReducerEvent {
                            reducer: super::Reducer::InsertTableHoldsTable { a, b },
                            ..
                        }),
                    ..
                } = ctx
                else {
                    unreachable!()
                };
                callback(ctx, a, b)
            }),
        ))
    }
    fn remove_on_insert_table_holds_table(&self, callback: InsertTableHoldsTableCallbackId) {
        self.imp.remove_on_reducer("insert_table_holds_table", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `insert_table_holds_table`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_insert_table_holds_table {
    /// Set the call-reducer flags for the reducer `insert_table_holds_table` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn insert_table_holds_table(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_insert_table_holds_table for super::SetReducerFlags {
    fn insert_table_holds_table(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("insert_table_holds_table", flags);
    }
}
