//! The [`DbContext`] trait, which mediates access to a remote module.
//!
//! [`DbContext`] is implemented by `DbConnection` and `EventContext`,
//! both defined in your module-specific codegen.

use crate::{Address, Identity};

pub trait DbContext {
    type DbView;

    /// Access to tables in the client cache, which stores a read-only replica of the remote database state.
    ///
    /// The returned `DbView` will have a method to access each table defined by the module.
    ///
    /// `DbConnection` and `EventContext` also have a public field `db`,
    /// so accesses to concrete-typed contexts don't need to use this method.
    fn db(&self) -> &Self::DbView;

    type Reducers;

    /// Access to reducers defined by the module.
    ///
    /// The returned `Reducers` will have a method to invoke each reducer defined by the module,
    /// plus methods for adding and removing callbacks on each of those reducers.
    ///
    /// `DbConnection` and `EventContext` also have a public field `reducers`,
    /// so accesses to concrete-typed contexts don't need to use this method.
    fn reducers(&self) -> &Self::Reducers;

    /// Returns `true` if the connection is active, i.e. has not yet disconnected.
    fn is_active(&self) -> bool;

    /// Close the connection.
    ///
    /// Returns an error if we are already disconnected.
    fn disconnect(&self) -> anyhow::Result<()>;

    type SubscriptionBuilder;
    /// Get a builder-pattern constructor for subscribing to queries,
    /// causing matching rows to be replicated into the client cache.
    fn subscription_builder(&self) -> Self::SubscriptionBuilder;

    /// Get the [`Identity`] of this connection.
    ///
    /// This method panics if the connection was constructed anonymously
    /// and we have not yet received our newly-generated [`Identity`] from the host.
    /// For a non-panicking version, see [`Self::try_identity`].
    fn identity(&self) -> Identity {
        self.try_identity().unwrap()
    }

    /// Get the [`Identity`] of this connection.
    ///
    /// This method returns `None` if the connection was constructed anonymously
    /// and we have not yet received our newly-generated [`Identity`] from the host.
    /// For a panicking version, see [`Self::identity`].
    fn try_identity(&self) -> Option<Identity>;

    /// Get this connection's client [`Address`].
    ///
    /// All connections opened by the same process will have the same [`Address`],
    /// including connections to different modules.
    fn address(&self) -> Address;
}
