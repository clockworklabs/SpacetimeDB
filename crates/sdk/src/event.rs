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

use crate::spacetime_module::{DbUpdate as _, SpacetimeModule};
use spacetimedb_client_api_messages::websocket as ws;
use spacetimedb_lib::{Address, Identity, Timestamp};

#[non_exhaustive]
#[derive(Debug)]
/// A change in the state of a [`crate::DbContext`] which causes callbacks to run.
pub enum Event<R> {
    /// Event when we are notified that a reducer ran in the remote module.
    ///
    /// This event is passed to reducer callbacks,
    /// and to row callbacks resulting from modifications by the reducer.
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
    SubscribeError(anyhow::Error),

    /// Event when we are notified of a transaction in the remote module which we cannot associate with a known reducer.
    ///
    /// This may be an ad-hoc SQL query or a reducer for which we do not have bindings.
    ///
    /// This event is passed to row callbacks resulting from modifications by the transaction.
    UnknownTransaction,
}

#[non_exhaustive]
#[derive(Debug)]
/// A state change due to a reducer, which may or may not have committed successfully.
pub struct ReducerEvent<R> {
    /// The time at which the reducer was invoked.
    pub timestamp: Timestamp,

    /// Whether the reducer committed, was aborted due to insufficient energy, or failed with an error message.
    pub status: Status,

    /// The `Identity` of the SpacetimeDB actor which invoked the reducer.
    pub caller_identity: Identity,

    /// The `Address` of the SpacetimeDB actor which invoked the reducer,
    /// or `None` if the actor did not supply an address.
    pub caller_address: Option<Address>,

    /// The amount of energy consumed by the reducer run, in eV.
    /// (Not literal eV, but our SpacetimeDB energy unit eV.)
    ///
    /// May be `None` if the module is configured not to broadcast energy consumed.
    pub energy_consumed: Option<u128>,

    /// The `Reducer` enum defined by the `module_bindings`, which encodes which reducer ran and its arguments.
    pub reducer: R,
}

#[derive(Debug, Clone)]
/// The termination status of a [`ReducerEvent`].
pub enum Status {
    /// The reducer terminated successfully, and its mutations were committed to the database's state.
    Committed,

    /// The reducer encountered an error during its execution, and its mutations were discarded or rolled back.
    ///
    /// The `String` payload is the error message signaled by the reducer,
    /// either as an `Err` return, a `panic` message, or a thrown exception.
    Failed(Box<str>),

    /// The reducer was aborted due to insufficient energy, and its mutations were discarded or rolled back.
    OutOfEnergy,
}

impl Status {
    pub(crate) fn parse_status_and_update<M: SpacetimeModule>(
        status: ws::UpdateStatus<ws::BsatnFormat>,
    ) -> anyhow::Result<(Self, Option<M::DbUpdate>)> {
        Ok(match status {
            ws::UpdateStatus::Committed(update) => (Self::Committed, Some(M::DbUpdate::parse_update(update)?)),
            ws::UpdateStatus::Failed(errmsg) => (Self::Failed(errmsg), None),
            ws::UpdateStatus::OutOfEnergy => (Self::OutOfEnergy, None),
        })
    }
}
