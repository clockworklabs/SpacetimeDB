//! Internal mechanisms for managing subscribed queries.
//!
//! This module is internal, and may incompatibly change without warning.

use crate::{
    db_connection::{next_request_id, next_subscription_id, DbContextImpl, PendingMutation},
    spacetime_module::{SpacetimeModule, SubscriptionHandle},
};
use anyhow::bail;
use futures_channel::mpsc;
use spacetimedb_client_api_messages::websocket::{self as ws};
use spacetimedb_data_structures::map::HashMap;
use std::sync::{atomic::AtomicU32, Arc, Mutex};

// TODO: Rewrite for subscription manipulation, once we get that.
// Currently race conditions abound, as you may resubscribe before the prev sub was applied,
// clobbering your previous callback.

pub struct SubscriptionManager<M: SpacetimeModule> {
    legacy_subscriptions: HashMap<u32, SubscribedQuery<M>>,
    new_subscriptions: HashMap<u32, SubscriptionHandleImpl<M>>,
}

impl<M: SpacetimeModule> Default for SubscriptionManager<M> {
    fn default() -> Self {
        Self {
            legacy_subscriptions: HashMap::default(),
            new_subscriptions: HashMap::default(),
        }
    }
}

pub(crate) type OnAppliedCallback<M> = Box<dyn FnOnce(&<M as SpacetimeModule>::EventContext) + Send + 'static>;
pub(crate) type OnErrorCallback<M> = Box<dyn FnOnce(&<M as SpacetimeModule>::EventContext) + Send + 'static>;
pub type OnEndedCallback<M> = Box<dyn FnOnce(&<M as SpacetimeModule>::EventContext) + Send + 'static>;

/// When handling a pending unsubscribe, there are three cases the caller must handle.
pub(crate) enum PendingUnsubscribeResult<M: SpacetimeModule> {
    // The unsubscribe message should be sent to the server.
    SendUnsubscribe(ws::Unsubscribe),
    // The subscription is immediately being cancelled, so the callback should be run.
    RunCallback(OnEndedCallback<M>),
    // No action is required.
    DoNothing,
}

impl<M: SpacetimeModule> SubscriptionManager<M> {
    pub(crate) fn on_disconnect(&mut self, ctx: &M::EventContext) {
        // We need to clear all the subscriptions.
        // We should run the on_ended callbacks for all of them.
        for (_, mut sub) in self.new_subscriptions.drain() {
            if let Some(callback) = sub.on_error() {
                callback(ctx);
            }
        }
        for (_, mut s) in self.legacy_subscriptions.drain() {
            if let Some(callback) = s.on_error.take() {
                callback(ctx);
            }
        }
    }

    /// Register a new subscription. This does not send the subscription to the server.
    /// Rather, it makes the subscription available for the next `apply_subscriptions` call.
    pub(crate) fn register_legacy_subscription(
        &mut self,
        sub_id: u32,
        on_applied: Option<OnAppliedCallback<M>>,
        on_error: Option<OnErrorCallback<M>>,
    ) {
        self.legacy_subscriptions
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

    pub(crate) fn legacy_subscription_applied(&mut self, ctx: &M::EventContext, sub_id: u32) {
        let sub = self.legacy_subscriptions.get_mut(&sub_id).unwrap();
        sub.is_applied = true;
        if let Some(callback) = sub.on_applied.take() {
            callback(ctx);
        }
    }

    /// Register a new subscription. This does not send the subscription to the server.
    /// Rather, it makes the subscription available for the next `apply_subscriptions` call.
    pub(crate) fn register_subscription(&mut self, query_id: u32, handle: SubscriptionHandleImpl<M>) {
        self.new_subscriptions
            .try_insert(query_id, handle.clone())
            .unwrap_or_else(|_| unreachable!("Duplicate subscription id {query_id}"));
    }

    /// This should be called when we get a subscription applied message from the server.
    pub(crate) fn subscription_applied(&mut self, ctx: &M::EventContext, sub_id: u32) {
        let Some(sub) = self.new_subscriptions.get_mut(&sub_id) else {
            // TODO: log or double check error handling.
            return;
        };
        if let Some(callback) = sub.on_applied() {
            callback(ctx)
        }
    }

    /// This should be called when we get a subscription applied message from the server.
    pub(crate) fn handle_pending_unsubscribe(&mut self, sub_id: u32) -> PendingUnsubscribeResult<M> {
        let Some(sub) = self.new_subscriptions.get(&sub_id) else {
            // TODO: log or double check error handling.
            return PendingUnsubscribeResult::DoNothing;
        };
        let mut sub = sub.clone();
        if sub.is_cancelled() {
            // This means that the subscription was cancelled before it was started.
            // We skip sending the subscription start message.
            self.new_subscriptions.remove(&sub_id);
            if let Some(callback) = sub.on_ended() {
                return PendingUnsubscribeResult::RunCallback(callback);
            } else {
                return PendingUnsubscribeResult::DoNothing;
            }
        }
        if sub.is_ended() {
            // This should only happen if the subscription was ended due to an error.
            // We don't need to send an unsubscribe message in this case.
            self.new_subscriptions.remove(&sub_id);
            return PendingUnsubscribeResult::DoNothing;
        }
        PendingUnsubscribeResult::SendUnsubscribe(ws::Unsubscribe {
            query_id: ws::QueryId::new(sub_id),
            request_id: next_request_id(),
        })
    }

    /// This should be called when we get an unsubscribe applied message from the server.
    pub(crate) fn unsubscribe_applied(&mut self, ctx: &M::EventContext, sub_id: u32) {
        let Some(mut sub) = self.new_subscriptions.remove(&sub_id) else {
            // TODO: double check error handling.
            log::debug!("Unsubscribe applied called for missing query {:?}", sub_id);
            return;
        };
        if let Some(callback) = sub.on_ended() {
            callback(ctx)
        }
    }

    /// This should be called when we get an unsubscribe applied message from the server.
    pub(crate) fn subscription_error(&mut self, ctx: &M::EventContext, sub_id: u32) {
        let Some(mut sub) = self.new_subscriptions.remove(&sub_id) else {
            // TODO: double check error handling.
            log::warn!("Unsubscribe applied called for missing query {:?}", sub_id);
            return;
        };
        if let Some(callback) = sub.on_error() {
            callback(ctx)
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

    pub fn subscribe(self, query_sql: &str) -> M::SubscriptionHandle {
        let qid = next_subscription_id();
        let handle = SubscriptionHandleImpl::new(SubscriptionState::new(
            qid,
            query_sql.into(),
            self.conn.pending_mutations_send.clone(),
            self.on_applied,
            self.on_error,
        ));
        self.conn
            .pending_mutations_send
            .unbounded_send(PendingMutation::SubscribeSingle {
                query_id: qid,
                handle: handle.clone(),
            })
            .unwrap();
        M::SubscriptionHandle::new(handle)
    }

    /// Subscribe to all rows from all tables.
    ///
    /// This method is intended as a convenience
    /// for applications where client-side memory use and network bandwidth are not concerns.
    /// Applications where these resources are a constraint
    /// should register more precise queries via [`Self::subscribe`]
    /// in order to replicate only the subset of data which the client needs to function.
    ///
    /// This method should not be combined with [`Self::subscribe`] on the same `DbConnection`.
    /// A connection may either [`Self::subscribe`] to particular queries,
    /// or [`Self::subscribe_to_all_tables`], but not both.
    /// Attempting to call [`Self::subscribe`]
    /// on a `DbConnection` that has previously used [`Self::subscribe_to_all_tables`],
    /// or vice versa, may misbehave in any number of ways,
    /// including dropping subscriptions, corrupting the client cache, or panicking.
    pub fn subscribe_to_all_tables(self) {
        static NEXT_SUB_ID: AtomicU32 = AtomicU32::new(0);

        let sub_id = NEXT_SUB_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let query = "SELECT * FROM *";

        let Self {
            on_applied,
            on_error,
            conn,
        } = self;
        conn.pending_mutations_send
            .unbounded_send(PendingMutation::Subscribe {
                on_applied,
                on_error,
                queries: [query].into_queries(),
                sub_id,
            })
            .unwrap();
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

/// This tracks what messages have been exchanged with the server.
#[derive(Debug, PartialEq, Eq, Clone)]
enum SubscriptionServerState {
    Pending, // This hasn't been sent to the server yet.
    Sent,    // We have sent it to the server.
    Applied, // The server has acknowledged it, and we are receiving updates.
    Ended,   // It has been unapplied.
    Error,   // There was an error that ended the subscription.
}

/// We track the state of a subscription here.
/// A reference to this is held by the `SubscriptionHandle` that clients use to unsubscribe,
/// and by the `SubscriptionManager` that handles updates from the server.
pub(crate) struct SubscriptionState<M: SpacetimeModule> {
    query_id: u32,
    query_sql: Box<str>,
    unsubscribe_called: bool,
    status: SubscriptionServerState,
    on_applied: Option<OnAppliedCallback<M>>,
    on_error: Option<OnErrorCallback<M>>,
    on_ended: Option<OnEndedCallback<M>>,
    // This is needed to schedule client operations.
    // Note that we shouldn't have a full connection here.
    pending_mutation_sender: mpsc::UnboundedSender<PendingMutation<M>>,
}

impl<M: SpacetimeModule> SubscriptionState<M> {
    pub(crate) fn new(
        query_id: u32,
        query_sql: Box<str>,
        pending_mutation_sender: mpsc::UnboundedSender<PendingMutation<M>>,
        on_applied: Option<OnAppliedCallback<M>>,
        on_error: Option<OnErrorCallback<M>>,
    ) -> Self {
        Self {
            query_id,
            query_sql,
            unsubscribe_called: false,
            status: SubscriptionServerState::Pending,
            on_applied,
            on_error,
            on_ended: None,
            pending_mutation_sender,
        }
    }

    /// Start the subscription.
    /// This updates the state in the handle, and returns the message to be sent to the server.
    /// The caller is responsible for sending the message to the server.
    pub(crate) fn start(&mut self) -> Option<ws::SubscribeSingle> {
        if self.unsubscribe_called {
            // This means that the subscription was cancelled before it was started.
            // We skip sending the subscription start message.
            return None;
        }
        if self.status != SubscriptionServerState::Pending {
            // This should never happen.
            // We should only start a subscription once.
            // If we are starting it again, we have a bug.
            unreachable!("Subscription already started");
        }
        self.status = SubscriptionServerState::Sent;
        Some(ws::SubscribeSingle {
            query_id: ws::QueryId::new(self.query_id),
            query: self.query_sql.clone(),
            request_id: next_request_id(),
        })
    }

    pub fn unsubscribe_then(&mut self, on_end: Option<OnEndedCallback<M>>) -> anyhow::Result<()> {
        // pub fn unsubscribe_then(&mut self, on_end: impl FnOnce(&M::EventContext) + Send + 'static) -> anyhow::Result<()> {
        if self.is_ended() {
            bail!("Subscription has already ended");
        }
        // Check if it has already been called.
        if self.unsubscribe_called {
            bail!("Unsubscribe already called");
        }

        self.unsubscribe_called = true;
        self.on_ended = on_end;
        // self.on_ended = Some(Box::new(on_end));

        // We send this even if the status is still Pending, so we can remove it from the manager.
        self.pending_mutation_sender
            .unbounded_send(PendingMutation::Unsubscribe {
                query_id: self.query_id,
            })
            .unwrap();
        Ok(())
    }

    /// Check if the client ended the subscription before we sent anything to the server.
    pub fn is_cancelled(&self) -> bool {
        self.status == SubscriptionServerState::Pending && self.unsubscribe_called
    }

    pub fn is_ended(&self) -> bool {
        matches!(
            self.status,
            SubscriptionServerState::Ended | SubscriptionServerState::Error
        )
    }

    pub fn is_active(&self) -> bool {
        match self.status {
            SubscriptionServerState::Applied => !self.unsubscribe_called,
            _ => false,
        }
    }

    pub fn on_applied(&mut self) -> Option<OnAppliedCallback<M>> {
        if self.status != SubscriptionServerState::Sent {
            // Potentially log a warning. This might make sense if we are shutting down.
            log::debug!(
                "on_applied called for query {:?} with status: {:?}",
                self.query_id,
                self.status
            );
            return None;
        }
        log::debug!("on_applied called for query {:?}", self.query_id);
        self.status = SubscriptionServerState::Applied;
        self.on_applied.take()
    }

    pub fn on_ended(&mut self) -> Option<OnAppliedCallback<M>> {
        // TODO: Consider logging a warning if the state is wrong (like being in the Error state).
        if self.is_ended() {
            return None;
        }
        self.status = SubscriptionServerState::Ended;
        self.on_ended.take()
    }

    pub fn on_error(&mut self) -> Option<OnErrorCallback<M>> {
        // TODO: Consider logging a warning if the state is wrong.
        if self.is_ended() {
            return None;
        }
        self.status = SubscriptionServerState::Error;
        self.on_error.take()
    }
}

#[doc(hidden)]
/// Internal implementation held by the module-specific generated `SubscriptionHandle` type.
pub struct SubscriptionHandleImpl<M: SpacetimeModule> {
    pub(crate) inner: Arc<Mutex<SubscriptionState<M>>>,
}

impl<M: SpacetimeModule> Clone for SubscriptionHandleImpl<M> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<M: SpacetimeModule> SubscriptionHandleImpl<M> {
    pub(crate) fn new(inner: SubscriptionState<M>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub(crate) fn start(&self) -> Option<ws::SubscribeSingle> {
        let mut inner = self.inner.lock().unwrap();
        inner.start()
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.inner.lock().unwrap().is_cancelled()
    }
    pub fn is_ended(&self) -> bool {
        self.inner.lock().unwrap().is_ended()
    }

    pub fn is_active(&self) -> bool {
        self.inner.lock().unwrap().is_active()
    }

    /// Called by the `SubscriptionHandle` method of the same name.
    pub fn unsubscribe_then(self, on_end: Option<OnEndedCallback<M>>) -> anyhow::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.unsubscribe_then(on_end)
    }

    /// Record that the subscription has been applied and return the callback to run.
    /// The caller is responsible for calling the callback.
    pub(crate) fn on_applied(&mut self) -> Option<OnAppliedCallback<M>> {
        let mut inner = self.inner.lock().unwrap();
        inner.on_applied()
    }

    /// Record that the subscription has been applied and return the callback to run.
    /// The caller is responsible for calling the callback.
    pub(crate) fn on_ended(&mut self) -> Option<OnEndedCallback<M>> {
        let mut inner = self.inner.lock().unwrap();
        inner.on_ended()
    }

    /// Record that the subscription has errored and return the callback to run.
    /// The caller is responsible for calling the callback.
    pub(crate) fn on_error(&mut self) -> Option<OnErrorCallback<M>> {
        let mut inner = self.inner.lock().unwrap();
        inner.on_error()
    }
}
