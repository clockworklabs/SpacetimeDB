// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused)]
use spacetimedb_sdk::{
    self as __sdk,
    anyhow::{self as __anyhow, Context as _},
    lib as __lib, sats as __sats, ws_messages as __ws,
};

use super::every_primitive_struct_type::EveryPrimitiveStruct;

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub struct InsertPrimitivesAsStrings {
    pub s: EveryPrimitiveStruct,
}

impl __sdk::spacetime_module::InModule for InsertPrimitivesAsStrings {
    type Module = super::RemoteModule;
}

pub struct InsertPrimitivesAsStringsCallbackId(__sdk::callbacks::CallbackId);

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
            .call_reducer("insert_primitives_as_strings", InsertPrimitivesAsStrings { s })
    }
    fn on_insert_primitives_as_strings(
        &self,
        mut callback: impl FnMut(&super::EventContext, &EveryPrimitiveStruct) + Send + 'static,
    ) -> InsertPrimitivesAsStringsCallbackId {
        InsertPrimitivesAsStringsCallbackId(self.imp.on_reducer::<InsertPrimitivesAsStrings>(
            "insert_primitives_as_strings",
            Box::new(move |ctx: &super::EventContext, args: &InsertPrimitivesAsStrings| callback(ctx, &args.s)),
        ))
    }
    fn remove_on_insert_primitives_as_strings(&self, callback: InsertPrimitivesAsStringsCallbackId) {
        self.imp
            .remove_on_reducer::<InsertPrimitivesAsStrings>("insert_primitives_as_strings", callback.0)
    }
}
