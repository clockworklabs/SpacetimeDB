use crate::spacetime_module::SpacetimeModule;
use anyhow::Context;
use spacetimedb_lib::{Address, Identity};
use std::time::SystemTime;

#[non_exhaustive]
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
pub struct ReducerEvent<R> {
    /// The time at which the reducer was invoked.
    pub timestamp: SystemTime,

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

pub enum Status {
    Committed,
    Failed(String),
    OutOfEnergy,
}

impl Status {
    pub(crate) fn parse_status_and_update<M: SpacetimeModule>(
        status: crate::ws_messages::UpdateStatus,
    ) -> anyhow::Result<(Self, Option<M::DbUpdate>)> {
        Ok(match status {
            crate::ws_messages::UpdateStatus::Committed(update) => (
                Self::Committed,
                Some(M::DbUpdate::try_from(update).context("Failed to parse DatabaseUpdate from UpdateStatus")?),
            ),
            crate::ws_messages::UpdateStatus::Failed(errmsg) => (Self::Failed(errmsg), None),
            crate::ws_messages::UpdateStatus::OutOfEnergy => (Self::OutOfEnergy, None),
        })
    }
}
