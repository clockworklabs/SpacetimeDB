// THIS FILE IS AUTOMATICALLY GENERATED BY SPACETIMEDB. EDITS TO THIS FILE
// WILL NOT BE SAVED. MODIFY TABLES IN RUST INSTEAD.

#![allow(unused, clippy::all)]
use spacetimedb_sdk::__codegen::{self as __sdk, __lib, __sats, __ws};

#[derive(__lib::ser::Serialize, __lib::de::Deserialize, Clone, PartialEq, Debug)]
#[sats(crate = __lib)]
pub(super) struct InitArgs {}

impl From<InitArgs> for super::Reducer {
    fn from(args: InitArgs) -> Self {
        Self::Init
    }
}

impl __sdk::InModule for InitArgs {
    type Module = super::RemoteModule;
}

pub struct InitCallbackId(__sdk::CallbackId);

#[allow(non_camel_case_types)]
/// Extension trait for access to the reducer `init`.
///
/// Implemented for [`super::RemoteReducers`].
pub trait init {
    /// Request that the remote module invoke the reducer `init` to run as soon as possible.
    ///
    /// This method returns immediately, and errors only if we are unable to send the request.
    /// The reducer will run asynchronously in the future,
    ///  and its status can be observed by listening for [`Self::on_init`] callbacks.
    fn init(&self) -> __sdk::Result<()>;
    /// Register a callback to run whenever we are notified of an invocation of the reducer `init`.
    ///
    /// The [`super::EventContext`] passed to the `callback`
    /// will always have [`__sdk::Event::Reducer`] as its `event`,
    /// but it may or may not have terminated successfully and been committed.
    /// Callbacks should inspect the [`__sdk::ReducerEvent`] contained in the [`super::EventContext`]
    /// to determine the reducer's status.
    ///
    /// The returned [`InitCallbackId`] can be passed to [`Self::remove_on_init`]
    /// to cancel the callback.
    fn on_init(&self, callback: impl FnMut(&super::EventContext) + Send + 'static) -> InitCallbackId;
    /// Cancel a callback previously registered by [`Self::on_init`],
    /// causing it not to run in the future.
    fn remove_on_init(&self, callback: InitCallbackId);
}

impl init for super::RemoteReducers {
    fn init(&self) -> __sdk::Result<()> {
        self.imp.call_reducer("init", InitArgs {})
    }
    fn on_init(&self, mut callback: impl FnMut(&super::EventContext) + Send + 'static) -> InitCallbackId {
        InitCallbackId(self.imp.on_reducer(
            "init",
            Box::new(move |ctx: &super::EventContext| {
                let super::EventContext {
                    event:
                        __sdk::Event::Reducer(__sdk::ReducerEvent {
                            reducer: super::Reducer::Init {},
                            ..
                        }),
                    ..
                } = ctx
                else {
                    unreachable!()
                };
                callback(ctx)
            }),
        ))
    }
    fn remove_on_init(&self, callback: InitCallbackId) {
        self.imp.remove_on_reducer("init", callback.0)
    }
}

#[allow(non_camel_case_types)]
#[doc(hidden)]
/// Extension trait for setting the call-flags for the reducer `init`.
///
/// Implemented for [`super::SetReducerFlags`].
///
/// This type is currently unstable and may be removed without a major version bump.
pub trait set_flags_for_init {
    /// Set the call-reducer flags for the reducer `init` to `flags`.
    ///
    /// This type is currently unstable and may be removed without a major version bump.
    fn init(&self, flags: __ws::CallReducerFlags);
}

impl set_flags_for_init for super::SetReducerFlags {
    fn init(&self, flags: __ws::CallReducerFlags) {
        self.imp.set_call_reducer_flags("init", flags);
    }
}
