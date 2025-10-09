//! Internal mechanisms for managing subscribed queries.
//!
//! This module is internal, and may incompatibly change without warning.

use crate::spacetime_module::AbstractEventContext;
use crate::{
    db_connection::{next_request_id, next_subscription_id, DbContextImpl, PendingMutation},
    spacetime_module::{SpacetimeModule, SubscriptionHandle},
};
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

pub(crate) type OnAppliedCallback<M> =
    Box<dyn FnOnce(&<M as SpacetimeModule>::SubscriptionEventContext) + Send + 'static>;
pub(crate) type OnErrorCallback<M> =
    Box<dyn FnOnce(&<M as SpacetimeModule>::ErrorContext, crate::Error) + Send + 'static>;
pub type OnEndedCallback<M> = Box<dyn FnOnce(&<M as SpacetimeModule>::SubscriptionEventContext) + Send + 'static>;

/// When handling a pending unsubscribe, there are three cases the caller must handle.
pub(crate) enum PendingUnsubscribeResult<M: SpacetimeModule> {
    // The unsubscribe message should be sent to the server.
    SendUnsubscribe(ws::UnsubscribeMulti),
    // The subscription is immediately being cancelled, so the callback should be run.
    RunCallback(OnEndedCallback<M>),
    // No action is required.
    DoNothing,
}

impl<M: SpacetimeModule> SubscriptionManager<M> {
    pub(crate) fn on_disconnect(&mut self, _ctx: &M::ErrorContext) {
        // We need to clear all the subscriptions.
        // TODO: is this correct? We don't remove them from the client cache,
        // we may want to resume them in the future if we impl reconnecting,
        // and users can already register on-disconnect callbacks which will run in this case.

        // NOTE(cloutiertyler)
        // This function previously invoke `on_error` for all subscriptions.
        // However, this is inconsistent behavior given that `on_disconnect` for
        // connections no longer always has an error argument and that the user
        // can add an `on_ended` callback when unsubscribing.
        //
        // We propose instead that `on_ended` be added to the subscription
        // builder so that it can be invoked when the subscription is ended
        // because of a normal disconnect, but without the user calling
        // `unsubscribe_then`. This can be done in a non-breaking way.
        //
        // For now, we will just do nothing when a subscription ends normally.
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

    pub(crate) fn legacy_subscription_applied(&mut self, ctx: &M::SubscriptionEventContext, sub_id: u32) {
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
    pub(crate) fn subscription_applied(&mut self, ctx: &M::SubscriptionEventContext, sub_id: u32) {
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
        PendingUnsubscribeResult::SendUnsubscribe(ws::UnsubscribeMulti {
            query_id: ws::QueryId::new(sub_id),
            request_id: next_request_id(),
        })
    }

    /// This should be called when we get an unsubscribe applied message from the server.
    pub(crate) fn unsubscribe_applied(&mut self, ctx: &M::SubscriptionEventContext, sub_id: u32) {
        let Some(mut sub) = self.new_subscriptions.remove(&sub_id) else {
            // TODO: double check error handling.
            log::debug!("Unsubscribe applied called for missing query {sub_id:?}");
            return;
        };
        if let Some(callback) = sub.on_ended() {
            callback(ctx)
        }
    }

    /// This should be called when we get an unsubscribe applied message from the server.
    pub(crate) fn subscription_error(&mut self, ctx: &M::ErrorContext, sub_id: u32) {
        let Some(mut sub) = self.new_subscriptions.remove(&sub_id) else {
            // TODO: double check error handling.
            log::warn!("Unsubscribe applied called for missing query {sub_id:?}");
            return;
        };
        if let Some(callback) = sub.on_error() {
            callback(ctx, ctx.event().clone().unwrap());
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
    pub fn on_applied(mut self, callback: impl FnOnce(&M::SubscriptionEventContext) + Send + 'static) -> Self {
        self.on_applied = Some(Box::new(callback));
        self
    }

    /// Register a callback to run when the subscription fails.
    ///
    /// Note that this callback may run either when attempting to apply the subscription,
    /// in which case [`Self::on_applied`] will never run,
    /// or later during the subscription's lifetime if the module's interface changes,
    /// in which case [`Self::on_applied`] may have already run.
    pub fn on_error(mut self, callback: impl FnOnce(&M::ErrorContext, crate::Error) + Send + 'static) -> Self {
        self.on_error = Some(Box::new(callback));
        self
    }

    pub fn subscribe<Queries: IntoQueries>(self, query_sql: Queries) -> M::SubscriptionHandle {
        let qid = next_subscription_id();
        let handle = SubscriptionHandleImpl::new(SubscriptionState::new(
            qid,
            query_sql.into_queries(),
            self.conn.pending_mutations_send.clone(),
            self.on_applied,
            self.on_error,
        ));
        self.conn
            .pending_mutations_send
            .unbounded_send(PendingMutation::SubscribeMulti {
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

/// Types which can be converted into a single query.
//
// This trait is necessary because of Rust's coherence rules.
// If you find and replace it with `Into<Box<str>>`,
// the compiler will complain on the `impl IntoQueries for [T; N]` impl
// that future updates may add `impl Into<Box<str>> for [T; N]`.
pub trait IntoQueryString {
    fn into_query_string(self) -> Box<str>;
}

macro_rules! impl_into_query_string_via_into {
    ($ty:ty $(, $tys:ty)* $(,)?) => {
        impl IntoQueryString for $ty {
            fn into_query_string(self) -> Box<str> {
                self.into()
            }
        }
        $(impl_into_query_string_via_into!($tys);)*
    };
}

impl_into_query_string_via_into! {
    &str, String, Box<str>,
}

/// Types which specify a list of query strings.
pub trait IntoQueries {
    fn into_queries(self) -> Box<[Box<str>]>;
}

impl<T: IntoQueryString> IntoQueries for T {
    fn into_queries(self) -> Box<[Box<str>]> {
        Box::new([self.into_query_string()])
    }
}

impl<T: IntoQueryString, const N: usize> IntoQueries for [T; N] {
    fn into_queries(self) -> Box<[Box<str>]> {
        self.into_iter().map(IntoQueryString::into_query_string).collect()
    }
}

impl<T: IntoQueryString + Clone> IntoQueries for &[T] {
    fn into_queries(self) -> Box<[Box<str>]> {
        self.iter().cloned().map(IntoQueryString::into_query_string).collect()
    }
}

impl<T: IntoQueryString> IntoQueries for Vec<T> {
    fn into_queries(self) -> Box<[Box<str>]> {
        self.into_iter().map(IntoQueryString::into_query_string).collect()
    }
}

impl IntoQueries for Box<[Box<str>]> {
    fn into_queries(self) -> Box<[Box<str>]> {
        self
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
    query_sql: Box<[Box<str>]>,
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
        query_sql: Box<[Box<str>]>,
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
    pub(crate) fn start(&mut self) -> Option<ws::SubscribeMulti> {
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
        Some(ws::SubscribeMulti {
            query_id: ws::QueryId::new(self.query_id),
            query_strings: self.query_sql.clone(),
            request_id: next_request_id(),
        })
    }

    pub fn unsubscribe_then(&mut self, on_end: Option<OnEndedCallback<M>>) -> crate::Result<()> {
        if self.is_ended() {
            return Err(crate::Error::AlreadyEnded);
        }
        // Check if it has already been called.
        if self.unsubscribe_called {
            return Err(crate::Error::AlreadyUnsubscribed);
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

    pub(crate) fn start(&self) -> Option<ws::SubscribeMulti> {
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
    pub fn unsubscribe_then(self, on_end: Option<OnEndedCallback<M>>) -> crate::Result<()> {
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

#[cfg(test)]
mod test {
    use super::*;

    #[allow(unused)]
    // Here to check that these statements compile.
    fn into_queries_box_str(query: Box<str>) {
        let _ = query.clone().into_query_string();
        let _ = <Box<str> as IntoQueryString>::into_query_string(query.clone());
        let _ = query.clone().into_queries();
        let _ = <[Box<str>; 1] as IntoQueries>::into_queries([query.clone()]);
        let _ = [query.clone()].into_queries();
        let slice: &[Box<str>] = std::slice::from_ref(&query);
        let _ = <&[Box<str>] as IntoQueries>::into_queries(slice);
        let _ = slice.into_queries();
        let _ = <Vec<Box<str>> as IntoQueries>::into_queries(vec![query.clone()]);
        let _ = vec![query.clone()].into_queries();
    }

    #[allow(unused)]
    // Here to check that these statements compile.
    fn into_queries_string(query: String) {
        let _ = query.clone().into_query_string();
        let _ = <String as IntoQueryString>::into_query_string(query.clone());
        let _ = query.clone().into_queries();
        let _ = <[String; 1] as IntoQueries>::into_queries([query.clone()]);
        let _ = [query.clone()].into_queries();
        let slice: &[String] = std::slice::from_ref(&query);
        let _ = <&[String] as IntoQueries>::into_queries(slice);
        let _ = slice.into_queries();
        let _ = <Vec<String> as IntoQueries>::into_queries(vec![query.clone()]);
        let _ = vec![query.clone()].into_queries();
    }

    #[allow(unused)]
    // Here to check that these statements compile.
    fn into_queries_str(query: &str) {
        let _ = query.into_query_string();
        let _ = <&str as IntoQueryString>::into_query_string(query);
        let _ = query.into_queries();
        let _ = <[&str; 1] as IntoQueries>::into_queries([query]);
        let _ = [query].into_queries();
        let slice: &[&str] = &[query];
        let _ = <&[&str] as IntoQueries>::into_queries(slice);
        let _ = slice.into_queries();
        let _ = <Vec<&str> as IntoQueries>::into_queries(vec![query]);
        let _ = vec![query].into_queries();
    }
}
