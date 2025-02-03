// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
    self as __sdk, __lib, __sats, __ws,
    anyhow::{self as __anyhow, Context as _},
};

use super::every_primitive_struct_type::EveryPrimitiveStruct;

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct InsertPrimitivesAsStringsArgs {
    pub s: EveryPrimitiveStruct,
}

impl From<InsertPrimitivesAsStringsArgs> for super::Reducer {
    fn from(args: InsertPrimitivesAsStringsArgs) -> Self {
        Self::InsertPrimitivesAsStrings { s: args.s }
    }
}

impl __sdk::InModule for InsertPrimitivesAsStringsArgs {
    type Module = super::RemoteModule;
}

pub struct InsertPrimitivesAsStringsCallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `insert_primitives_as_strings`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait insert_primitives_as_strings {
    /// Request that the remote module invoke the reducer `insert_primitives_as_strings` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_insert_primitives_as_strings`] callbacks.
    fn insert_primitives_as_strings(&self, s: EveryPrimitiveStruct) -> __anyhow::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `insert_primitives_as_strings`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InsertPrimitivesAsStringsCallbackId`] can be passed to [`Self::remove_on_insert_primitives_as_strings`]
    /// to cancel the callback.
    fn on_insert_primitives_as_strings(
        &self,
        callback: impl FnMut(&super::EventContext, &EveryPrimitiveStruct) + Send + 'static,
    ) -> InsertPrimitivesAsStringsCallbackId;
    /// Cancel a callback previously registered by [`Self::on_insert_primitives_as_strings`],
    /// causing it not to run in the future.
    fn remove_on_insert_primitives_as_strings(&self, callback: InsertPrimitivesAsStringsCallbackId);
}

impl insert_primitives_as_strings for super::RemoteReducers {
    fn insert_primitives_as_strings(&self, s: EveryPrimitiveStruct) -> __anyhow::Result<()> {
        self.imp
            .call_reducer("insert_primitives_as_strings", InsertPrimitivesAsStringsArgs { s })
    }
    fn on_insert_primitives_as_strings(
        &self,
        mut callback: impl FnMut(&super::EventContext, &EveryPrimitiveStruct) + Send + 'static,
    ) -> InsertPrimitivesAsStringsCallbackId {
        InsertPrimitivesAsStringsCallbackId(self.imp.on_reducer(
            "insert_primitives_as_strings",
            Box::new(move |ctx: &super::EventContext| {
                let super::EventContext {
                    event:
                        __sdk::Event::Reducer(__sdk::ReducerEvent {
                            reducer: super::Reducer::InsertPrimitivesAsStrings { s },
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
    fn remove_on_insert_primitives_as_strings(&self, callback: InsertPrimitivesAsStringsCallbackId) {
        self.imp.remove_on_reducer("insert_primitives_as_strings", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `insert_primitives_as_strings`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_insert_primitives_as_strings {
    /// Set the call-reducer flags for the reducer `insert_primitives_as_strings` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn insert_primitives_as_strings(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_insert_primitives_as_strings for super::SetReducerFlags {
    fn insert_primitives_as_strings(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("insert_primitives_as_strings", flags);
    }
}
