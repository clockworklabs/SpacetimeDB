//! The [`Event`] enum, which encodes the different things that can happen
//! to cause callbacks to run with an `EventContext`.
//!
//! The SpacetimeDB per-module bindings will define a struct `EventContext`,
//! representing a connection to a remote database during some particular event.
//! That `EventContext` struct will have a field `event`, whose type is `Event<Reducer>`,
//! where [`Event`] is defined here and `Reducer` is an enum defined by the per-module bindings.
//!
//! Each callback invoked by the SpacetimeDB SDK will receive an `EventContext` as an argument.
//! You can inspect its `event` field
//! to determine what change in your connection's state caused the callback to run.

use spacetimedb_lib::Timestamp;

use crate::error::InternalError;

#[non_exhaustive]
#[derive(Debug, Clone)]
/// A change in the state of a [`crate::DbContext`] which causes callbacks to run.
pub enum Event<R> {
    /// Event when we are notified that a reducer this client invoked
    /// ran to completion in the remote module and its mutations were committed.
    ///
    /// This event is passed to row callbacks resulting from modifications by the reducer.
    Reducer(ReducerEvent<R>),

    /// Event when one of our subscriptions is applied.
    ///
    /// This event is passed to subscription-applied callbacks,
    /// and to row insert callbacks resulting from the new subscription.
    SubscribeApplied,

    /// Event when one of our subscriptions is removed.
    ///
    /// This event is passed to unsubscribe-applied callbacks,
    /// and to row delete callbacks resulting from the ended subscription.
    UnsubscribeApplied,

    /// Event when a subscription was ended by a disconnection.
    Disconnected,

    /// Event when an error causes one or more of our subscriptions to end prematurely,
    /// or to never be started.
    ///
    /// Payload should be a language-appropriate dynamic error type,
    /// likely `Exception` in C# and `Error` in TypeScript.
    ///
    /// Payload should describe the error in a human-readable format.
    /// No requirement is imposed that it be programmatically inspectable.
    SubscribeError(crate::Error),

    /// Event when we are notified of a transaction in the remote module,
    /// other than one that was the result of a reducer invoked by this client.
    ///
    /// Transactions resulting from reducers invoked by this client will instead report [`Event::Reducer`].
    ///
    /// This event is passed to row callbacks resulting from modifications by the transaction.
    Transaction,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
/// A state change due to a reducer, which may or may not have committed successfully.
pub struct ReducerEvent<R> {
    /// The time at which the reducer was invoked.
    pub timestamp: Timestamp,

    /// Whether the reducer committed, rolled back by returning an error, or was aborted by the host.
    pub status: Status,

    /// The `Reducer` enum defined by the `module_bindings`, which encodes which reducer ran and its arguments.
    pub reducer: R,
}

#[derive(Debug, Clone)]
/// The termination status of a [`ReducerEvent`].
pub enum Status {
    /// The reducer terminated successfully, and its mutations were committed to the database's state.
    Committed,

    /// The reducer returned or threw a handleable error and its mutations were rolled back.
    ///
    /// The `String` payload is the error message signaled by the reducer,
    /// either as an `Err` return, a `panic` message, or a thrown exception.
    Err(String),

    /// The reducer was aborted due to an unexpected or exceptional circumstance.
    Panic(InternalError),
}
