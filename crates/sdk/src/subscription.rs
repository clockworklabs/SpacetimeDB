//! Internal mechanisms for managing subscribed queries.
//!
//! This module is internal, and may incompatibly change without warning.

use crate::{
    db_connection::{DbContextImpl, PendingMutation},
    spacetime_module::{SpacetimeModule, SubscriptionHandle},
};
use spacetimedb_data_structures::map::HashMap;
use std::sync::atomic::AtomicU32;

// TODO: Rewrite for subscription manipulation, once we get that.
// Currently race conditions abound, as you may resubscribe before the prev sub was applied,
// clobbering your previous callback.

pub struct SubscriptionManager<M: SpacetimeModule> {
    subscriptions: HashMap<u32, SubscribedQuery<M>>,
}

impl<M: SpacetimeModule> Default for SubscriptionManager<M> {
    fn default() -> Self {
        Self {
            subscriptions: HashMap::default(),
        }
    }
}

pub(crate) type OnAppliedCallback<M> = Box<dyn FnOnce(&<M as SpacetimeModule>::EventContext) + Send + 'static>;
pub(crate) type OnErrorCallback<M> = Box<dyn FnOnce(&<M as SpacetimeModule>::EventContext) + Send + 'static>;

impl<M: SpacetimeModule> SubscriptionManager<M> {
    pub(crate) fn register_subscription(
        &mut self,
        sub_id: u32,
        on_applied: Option<OnAppliedCallback<M>>,
        on_error: Option<OnErrorCallback<M>>,
    ) {
        self.subscriptions
            .try_insert(
                sub_id,
                SubscribedQuery {
                    on_applied,
                    on_error,
                    is_applied: false,
                },
            )
            .unwrap_or_else(|_| unreachable!("Duplicate subscription id {sub_id}"));
    }
    pub(crate) fn subscription_applied(&mut self, ctx: &M::EventContext, sub_id: u32) {
        let sub = self.subscriptions.get_mut(&sub_id).unwrap();
        sub.is_applied = true;
        if let Some(callback) = sub.on_applied.take() {
            callback(ctx);
        }
    }
}

struct SubscribedQuery<M: SpacetimeModule> {
    on_applied: Option<OnAppliedCallback<M>>,
    #[allow(unused)]
    on_error: Option<OnErrorCallback<M>>,
    is_applied: bool,
}

/// Builder-pattern constructor for subscription queries.
///
/// This interface will change in an upcoming SpacetimeDB release
/// to support adding and removing individual queries to and from a client's subscriptions.
// TODO: Move into a different module which is not #[doc(hidden)]?
pub struct SubscriptionBuilder<M: SpacetimeModule> {
    on_applied: Option<OnAppliedCallback<M>>,
    on_error: Option<OnErrorCallback<M>>,
    conn: DbContextImpl<M>,
}

impl<M: SpacetimeModule> SubscriptionBuilder<M> {
    #[doc(hidden)]
    /// Call `ctx.subscription_builder()` instead.
    pub fn new(imp: &DbContextImpl<M>) -> Self {
        Self {
            on_applied: None,
            on_error: None,
            conn: imp.clone(),
        }
    }

    /// Register a callback to run when the subscription is applied.
    pub fn on_applied(mut self, callback: impl FnOnce(&M::EventContext) + Send + 'static) -> Self {
        self.on_applied = Some(Box::new(callback));
        self
    }

    /// Register a callback to run when the subscription fails.
    ///
    /// Note that this callback may run either when attempting to apply the subscription,
    /// in which case [`Self::on_applied`] will never run,
    /// or later during the subscription's lifetime if the module's interface changes,
    /// in which case [`Self::on_applied`] may have already run.
    // Currently unused. Hooking this up requires the new subscription interface and WS protocol.
    pub fn on_error(mut self, callback: impl FnOnce(&M::EventContext) + Send + 'static) -> Self {
        self.on_error = Some(Box::new(callback));
        self
    }

    /// Subscribe to `queries`, which should be a collection of SQL queries,
    /// each of which is a single-table non-projected `SELECT` statement
    /// with an optional `WHERE` clause,
    /// and `JOIN`ed with at most one other table as a filter.
    pub fn subscribe(self, queries: impl IntoQueries) -> M::SubscriptionHandle {
        static NEXT_SUB_ID: AtomicU32 = AtomicU32::new(0);

        let sub_id = NEXT_SUB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let Self {
            on_applied,
            on_error,
            conn,
        } = self;
        conn.pending_mutations_send
            .unbounded_send(PendingMutation::Subscribe {
                on_applied,
                on_error,
                queries: queries.into_queries(),
                sub_id,
            })
            .unwrap();
        M::SubscriptionHandle::new(SubscriptionHandleImpl { conn, sub_id })
    }
}

/// Types which specify a list of query strings.
pub trait IntoQueries {
    /// Convert into the list of queries.
    fn into_queries(self) -> Box<[Box<str>]>;
}

impl IntoQueries for Box<[Box<str>]> {
    fn into_queries(self) -> Box<[Box<str>]> {
        self
    }
}

impl<S: Copy + Into<Box<str>>> IntoQueries for &[S] {
    fn into_queries(self) -> Box<[Box<str>]> {
        self.iter().copied().map(Into::into).collect()
    }
}

impl<S: Into<Box<str>>, const N: usize> IntoQueries for [S; N] {
    fn into_queries(self) -> Box<[Box<str>]> {
        self.map(Into::into).into()
    }
}

#[doc(hidden)]
/// Internal implementation held by the module-specific generated `SubscriptionHandle` type.
pub struct SubscriptionHandleImpl<M: SpacetimeModule> {
    conn: DbContextImpl<M>,
    #[allow(unused)]
    sub_id: u32,
}

impl<M: SpacetimeModule> SubscriptionHandleImpl<M> {
    /// Called by the `SubscriptionHandle` method of the same name.
    pub fn is_ended(&self) -> bool {
        // When a subscription ends, we remove its `SubscribedQuery` from the `SubscriptionManager`.
        // So, to check if a subscription has ended, we check if the entry is present.
        // TODO: Note that we never end a subscription currently.
        // This will change with the implementation of the subscription management proposal.
        !self
            .conn
            .inner
            .lock()
            .unwrap()
            .subscriptions
            .subscriptions
            .contains_key(&self.sub_id)
    }

    /// Called by the `SubscriptionHandle` method of the same name.
    pub fn is_active(&self) -> bool {
        // A subscription is active if:
        // - It has not yet ended, i.e. is still present in the `SubscriptionManager`.
        // - It has been applied.
        self.conn
            .inner
            .lock()
            .unwrap()
            .subscriptions
            .subscriptions
            .get(&self.sub_id)
            .map(|sub| sub.is_applied)
            .unwrap_or(false)
    }

    /// Called by the `SubscriptionHandle` method of the same name.
    pub fn unsubscribe(self) -> anyhow::Result<()> {
        self.unsubscribe_then(|_| {})
    }

    /// Called by the `SubscriptionHandle` method of the same name.
    // TODO: requires the new subscription interface and WS protocol.
    pub fn unsubscribe_then(self, _on_end: impl FnOnce(&M::EventContext) + Send + 'static) -> anyhow::Result<()> {
        todo!()
    }
}
