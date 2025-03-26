// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN YOUR MODULE SOURCE CODE INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{
	self as __sdk,
	__lib,
	__sats,
	__ws,
};


#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct InsertVecI16Args {
    pub n: Vec::<i16>,
}

impl From<InsertVecI16Args> for super::Reducer {
    fn from(args: InsertVecI16Args) -> Self {
        Self::InsertVecI16 {
            n: args.n,
}
}
}

impl __sdk::InModule for InsertVecI16Args {
    type Module = super::RemoteModule;
}

pub struct InsertVecI16CallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `insert_vec_i16`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait insert_vec_i_16 {
    /// Request that the remote module invoke the reducer `insert_vec_i16` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_insert_vec_i_16`] callbacks.
    fn insert_vec_i_16(&self, n: Vec::<i16>,
) -> __sdk::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `insert_vec_i16`.
    ///
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::ReducerEventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InsertVecI16CallbackId`] can be passed to [`Self::remove_on_insert_vec_i_16`]
    /// to cancel the callback.
    fn on_insert_vec_i_16(&self, callback: impl FnMut(&super::ReducerEventContext, &Vec::<i16>, ) + Send + 'static) -> InsertVecI16CallbackId;
    /// Cancel a callback previously registered by [`Self::on_insert_vec_i_16`],
    /// causing it not to run in the future.
    fn remove_on_insert_vec_i_16(&self, callback: InsertVecI16CallbackId);
}

impl insert_vec_i_16 for super::RemoteReducers {
    fn insert_vec_i_16(&self, n: Vec::<i16>,
) -> __sdk::Result<()> {
        self.imp.call_reducer("insert_vec_i16", InsertVecI16Args { n,  })
    }
    fn on_insert_vec_i_16(
        &self,
        mut callback: impl FnMut(&super::ReducerEventContext, &Vec::<i16>, ) + Send + 'static,
    ) -> InsertVecI16CallbackId {
        InsertVecI16CallbackId(self.imp.on_reducer(
            "insert_vec_i16",
            Box::new(move |ctx: &super::ReducerEventContext| {
                let super::ReducerEventContext {
                    event: __sdk::ReducerEvent {
                        reducer: super::Reducer::InsertVecI16 {
                            n, 
                        },
                        ..
                    },
                    ..
                } = ctx else { unreachable!() };
                callback(ctx, n, )
            }),
        ))
    }
    fn remove_on_insert_vec_i_16(&self, callback: InsertVecI16CallbackId) {
        self.imp.remove_on_reducer("insert_vec_i16", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `insert_vec_i16`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_insert_vec_i_16 {
    /// Set the call-reducer flags for the reducer `insert_vec_i16` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn insert_vec_i_16(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_insert_vec_i_16 for super::SetReducerFlags {
    fn insert_vec_i_16(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("insert_vec_i16", flags);
    }
}

