//! Internal implementations of connections to a remote database.
//!
//! Contains a whole bunch of stuff that is referenced by the CLI codegen,
//! most notably [`DbContextImpl`], which implements `DbConnection` and `EventContext`.
//!
//! Broadly speaking, the Rust SDK works by having a background Tokio worker [`WsConnection`]
//! send and receive raw messages.
//! Incoming messages are then parsed by the [`parse_loop`] into domain types in [`ParsedMessage`],
//! which are processed and applied to the client cache state
//! when a user calls `DbConnection::advance_one_message` or its friends.
//!
//! Callbacks may access the database context through an `EventContext`,
//! and may therefore add or remove callbacks on the same or other events,
//! query the client cache, add or remove subscriptions, and make many other mutations.
//! To prevent deadlocks or re-entrancy, the SDK arranges to defer all such mutations in a queue
//! called`pending_mutations`, which are processed and applied during `advance_one_message`,
//! as with received WebSocket messages.
//!
//! This module is internal, and may incompatibly change without warning.

use crate::{
    __codegen::{InternalError, Reducer},
    callbacks::{
        CallbackId, DbCallbacks, ProcedureCallback, ProcedureCallbacks, ReducerCallback, ReducerCallbacks, RowCallback,
        UpdateCallback,
    },
    client_cache::{ClientCache, TableHandle},
    spacetime_module::{AbstractEventContext, AppliedDiff, DbConnection, DbUpdate, InModule, SpacetimeModule},
    subscription::{PendingUnsubscribeResult, SubscriptionHandleImpl, SubscriptionManager},
    websocket::{WsConnection, WsParams},
    AutomaticReconnectOptions, Event, NextReconnect, ReducerEvent, Status, TokenProvider,
};
use bytes::Bytes;
use futures::{
    future::{AbortHandle, Abortable},
    StreamExt,
};

#[cfg(feature = "browser")]
use futures::{pin_mut, FutureExt};
use futures_channel::mpsc;
use http::Uri;
use spacetimedb_client_api_messages::websocket::{self as ws, common::QuerySetId};
use spacetimedb_lib::{bsatn, ser::Serialize, ConnectionId, Identity, Timestamp};
use spacetimedb_sats::Deserialize;
#[cfg(not(feature = "browser"))]
use std::fs::OpenOptions;
use std::{
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{atomic::AtomicU32, Arc, Mutex as StdMutex, OnceLock},
};
#[cfg(not(feature = "browser"))]
use tokio::{
    runtime::{self, Runtime},
    sync::Mutex as TokioMutex,
};

pub(crate) type SharedCell<T> = Arc<StdMutex<T>>;

#[cfg(not(feature = "browser"))]
type SharedAsyncCell<T> = Arc<TokioMutex<T>>;
#[cfg(feature = "browser")]
type SharedAsyncCell<T> = SharedCell<T>;

/// Implementation of `DbConnection`, `EventContext`,
/// and anything else that provides access to the database connection.
///
/// This must be relatively cheaply `Clone`-able, and have internal sharing,
/// as numerous operations will clone it to get new handles on the connection.
pub struct DbContextImpl<M: SpacetimeModule> {
    #[cfg(not(feature = "browser"))]
    runtime: runtime::Handle,

    /// All the state which is safe to hold a lock on while running callbacks.
    pub(crate) inner: SharedCell<DbContextImplInner<M>>,

    /// None if we have disconnected.
    pub(crate) send_chan: SharedCell<Option<mpsc::UnboundedSender<ws::v2::ClientMessage>>>,

    /// The client cache, which stores subscribed rows.
    cache: SharedCell<ClientCache<M>>,

    /// Receiver channel for WebSocket messages,
    /// which are pre-parsed in the background by [`parse_loop`].
    recv: SharedAsyncCell<mpsc::UnboundedReceiver<ParsedMessage<M>>>,

    /// Channel into which operations which apparently mutate SDK state,
    /// e.g. registering callbacks, push [`PendingMutation`] messages,
    /// rather than immediately locking the connection and applying their change,
    /// to avoid deadlocks and races.
    pub(crate) pending_mutations_send: mpsc::UnboundedSender<PendingMutation<M>>,

    /// Receive end of `pending_mutations_send`,
    /// from which [Self::apply_pending_mutations] and friends read mutations.
    pending_mutations_recv: SharedAsyncCell<mpsc::UnboundedReceiver<PendingMutation<M>>>,

    /// This connection's `Identity`.
    ///
    /// May be `None` if we connected anonymously
    /// and have not yet received the [`ws::v2::InitialConnection`] message.
    identity: SharedCell<Option<Identity>>,

    /// This connection's `ConnectionId`.
    ///
    /// This may be none if we have not yet received the [`ws::v2::InitialConnection`] message.
    connection_id: SharedCell<Option<ConnectionId>>,

    reconnect: SharedCell<ReconnectState>,
    pub(crate) extra_logging: Option<SharedCell<File>>,
}

impl<M: SpacetimeModule> Clone for DbContextImpl<M> {
    fn clone(&self) -> Self {
        Self {
            #[cfg(not(feature = "browser"))]
            runtime: self.runtime.clone(),
            // Being very explicit with `Arc::clone` here,
            // since we'll be doing `DbContextImpl::clone` very frequently,
            // and we need it to be fast.
            inner: Arc::clone(&self.inner),
            send_chan: Arc::clone(&self.send_chan),
            cache: Arc::clone(&self.cache),
            recv: Arc::clone(&self.recv),
            pending_mutations_send: self.pending_mutations_send.clone(),
            pending_mutations_recv: Arc::clone(&self.pending_mutations_recv),
            identity: Arc::clone(&self.identity),
            connection_id: Arc::clone(&self.connection_id),
            extra_logging: Option::<Arc<_>>::clone(&self.extra_logging),
            reconnect: self.reconnect.clone(),
        }
    }
}

impl<M: SpacetimeModule> DbContextImpl<M> {
    pub(crate) fn debug_log(&self, body: impl FnOnce(&mut File) -> std::result::Result<(), std::io::Error>) {
        debug_log(&self.extra_logging, body);
    }

    /// Process a parsed WebSocket message,
    /// applying its mutations to the client cache and invoking callbacks.
    fn process_message(&self, msg: ParsedMessage<M>) -> crate::Result<()> {
        let result = self.process_message_inner(msg);
        let ended = self.inner.lock().unwrap().connection_lifecycle == ConnectionLifecycle::Ended;
        match result {
            Err(crate::Error::Disconnected) if !ended => self.connection_lost(None, false, false),
            Err(error) if !ended => self.connection_lost(Some(error), true, false),
            result => result,
        }
    }

    fn process_message_inner(&self, msg: ParsedMessage<M>) -> crate::Result<()> {
        // Discard queued messages after explicit disconnect or a terminal failure.
        if self.inner.lock().unwrap().connection_lifecycle == ConnectionLifecycle::Ended {
            return Err(crate::Error::Disconnected);
        }
        self.debug_log(|out| writeln!(out, "`process_message`: {msg:?}"));
        match msg {
            // Error: route as a connection error if we never finished connecting,
            // otherwise treat it as an erroneous disconnect.
            ParsedMessage::Error(e) => self.connection_lost(Some(e), true, false),
            ParsedMessage::TransportError(e) => {
                let terminal = e.is_terminal();
                let auth = e.is_auth();
                self.connection_lost(
                    Some(InternalError::new(e.to_string()).with_cause(e).into()),
                    terminal,
                    auth,
                )
            }
            ParsedMessage::IdentityToken(identity, token, conn_id) => {
                if self.inner.lock().unwrap().connection_lifecycle == ConnectionLifecycle::Ended {
                    return Ok(());
                }
                if self
                    .identity
                    .lock()
                    .unwrap()
                    .is_some_and(|previous| previous != identity)
                {
                    return self.connection_lost(
                        Some(InternalError::new("Reconnect returned a different identity").into()),
                        true,
                        false,
                    );
                }
                let reconnecting = {
                    let mut state = self.reconnect.lock().unwrap();
                    let reconnecting = state.reconnecting;
                    state.reconnecting = false;
                    state.attempt = 0;
                    state.token = Some(token.to_string());
                    reconnecting
                };
                *self.identity.lock().unwrap() = Some(identity);
                *self.connection_id.lock().unwrap() = Some(conn_id);
                {
                    let mut inner = self.inner.lock().unwrap();
                    if inner.connection_lifecycle == ConnectionLifecycle::Connected {
                        drop(inner);
                        return self.connection_lost(
                            Some(InternalError::new("Duplicate InitialConnection").into()),
                            true,
                            false,
                        );
                    }
                    inner.connection_lifecycle = ConnectionLifecycle::Connected;
                }
                let ctx = <M::DbConnection as DbConnection>::new(self.clone());
                if reconnecting {
                    let callback = self.inner.lock().unwrap().on_automatic_reconnect.take();
                    if let Some(mut callback) = callback {
                        callback(&ctx, identity, &token);
                        self.inner.lock().unwrap().on_automatic_reconnect = Some(callback);
                    }
                } else {
                    let callback = self.inner.lock().unwrap().on_connect.take();
                    if let Some(callback) = callback {
                        callback(&ctx, identity, &token);
                    }
                }
                if reconnecting {
                    // Include subscriptions queued from the reconnect callback in the batch.
                    self.reconnect.lock().unwrap().preparing_replay = true;
                    self.apply_pending_mutations()?;
                    if self.inner.lock().unwrap().connection_lifecycle != ConnectionLifecycle::Connected {
                        return Ok(());
                    }
                    let mut inner = self.inner.lock().unwrap();
                    let sets = inner.subscriptions.replay(&self.make_event_ctx(()));
                    let request_id = next_request_id();
                    inner.replay = Some((request_id, sets.iter().map(|set| set.query_set_id).collect()));
                    self.reconnect.lock().unwrap().preparing_replay = false;
                    drop(inner);
                    if sets.is_empty() {
                        self.apply_replay(ws::v2::SubscribeBatchApplied {
                            request_id,
                            results: Box::new([]),
                        })?;
                    } else {
                        self.send_message(ws::v2::ClientMessage::SubscribeBatch(ws::v2::SubscribeBatch {
                            request_id,
                            sets,
                        }))?;
                    }
                }
                Ok(())
            }
            ParsedMessage::SubscribeBatchApplied(batch) => self.apply_replay(batch),

            // Transaction update:
            // apply the received diff to the client cache,
            // then invoke row callbacks.
            ParsedMessage::TransactionUpdate(update) => {
                self.apply_update(update, |_| Event::Transaction);
                Ok(())
            }

            // Successful reducer run:
            // apply the received diff to the client cache,
            // construct an event with the reducer information,
            // then invoke row callbacks and the reducer's callback.
            ParsedMessage::ReducerResult {
                request_id,
                timestamp,
                result: Ok(Ok(update)),
            } => {
                let (reducer, callback) = {
                    let mut inner = self.inner.lock().unwrap();
                    inner.reducer_callbacks.pop_call_info(request_id).ok_or_else(|| {
                        InternalError::new(format!("Reducer result for unknown request_id {request_id}"))
                    })?
                };
                let reducer_event = ReducerEvent {
                    reducer,
                    timestamp,
                    status: Status::Committed,
                };

                self.apply_update(update, |_| Event::Reducer(reducer_event.clone()));

                let reducer_event_ctx = self.make_event_ctx(reducer_event);
                callback(&reducer_event_ctx, Ok(Ok(())));
                Ok(())
            }

            // Failed reducer run (note that previous pattern excludes `result: Ok(Ok(_))`):
            // construct an event with the reducer information,
            // then invoke the reducer's callback.
            ParsedMessage::ReducerResult {
                request_id,
                timestamp,
                result,
            } => {
                let (status, result) = match result {
                    Ok(Ok(_)) => {
                        unreachable!("This pattern handled by an earlier branch in the match on the `ParsedMessage`")
                    }
                    Ok(Err(message)) => (Status::Err(message.clone()), Ok(Err(message))),
                    Err(internal_error) => (Status::Panic(internal_error.clone()), Err(internal_error)),
                };
                let (reducer, callback) = {
                    let mut inner = self.inner.lock().unwrap();
                    inner.reducer_callbacks.pop_call_info(request_id).ok_or_else(|| {
                        InternalError::new(format!("Reducer result for unknown request_id {request_id}"))
                    })?
                };

                let reducer_event = ReducerEvent {
                    reducer,
                    timestamp,
                    status,
                };

                let reducer_event_ctx = self.make_event_ctx(reducer_event);
                callback(&reducer_event_ctx, result);
                Ok(())
            }

            ParsedMessage::SubscribeApplied {
                query_set_id,
                initial_update,
            } => {
                self.apply_update(initial_update, |inner| {
                    let sub_event_ctx = self.make_event_ctx(());
                    inner.subscriptions.subscription_applied(&sub_event_ctx, query_set_id);
                    Event::SubscribeApplied
                });
                Ok(())
            }
            ParsedMessage::UnsubscribeApplied {
                query_set_id,
                initial_update,
            } => {
                self.apply_update(initial_update, |inner| {
                    let sub_event_ctx = self.make_event_ctx(());
                    inner.subscriptions.unsubscribe_applied(&sub_event_ctx, query_set_id);
                    Event::UnsubscribeApplied
                });
                Ok(())
            }
            ParsedMessage::SubscriptionError { query_set_id, error } => {
                let error = crate::Error::SubscriptionError { error };
                let ctx = self.make_event_ctx(Some(error));
                let mut inner = self.inner.lock().unwrap();
                inner.subscriptions.subscription_error(&ctx, query_set_id);
                Ok(())
            }
            ParsedMessage::ProcedureResult { request_id, result } => {
                let ctx = self.make_event_ctx(());
                self.inner
                    .lock()
                    .unwrap()
                    .procedure_callbacks
                    .resolve(&ctx, request_id, result);
                Ok(())
            }
        }
    }

    fn apply_update(
        &self,
        update: M::DbUpdate,
        get_event: impl FnOnce(&mut DbContextImplInner<M>) -> Event<M::Reducer>,
    ) {
        // Lock the client cache in a restricted scope,
        // so that it will be unlocked when callbacks run.
        let applied_diff = {
            let mut cache = self.cache.lock().unwrap();
            update.apply_to_client_cache(&mut *cache)
        };
        let mut inner = self.inner.lock().unwrap();

        let event = get_event(&mut inner);
        let row_event_ctx = self.make_event_ctx(event);
        applied_diff.invoke_row_callbacks(&row_event_ctx, &mut inner.db_callbacks);
    }

    fn send_message(&self, message: ws::v2::ClientMessage) -> crate::Result<()> {
        self.send_chan
            .lock()
            .unwrap()
            .as_ref()
            .ok_or(crate::Error::Disconnected)?
            .unbounded_send(message)
            .map_err(|_| crate::Error::Disconnected)
    }

    fn apply_replay(&self, batch: ws::v2::SubscribeBatchApplied) -> crate::Result<()> {
        let expected = self.inner.lock().unwrap().replay.take();
        let ids: Vec<_> = batch.results.iter().map(|result| result.query_set_id).collect();
        if expected != Some((batch.request_id, ids)) {
            return self.connection_lost(
                Some(InternalError::new("Unexpected subscription replay response").into()),
                true,
                false,
            );
        }
        let mut tables = self.cache.lock().unwrap().removal_snapshot();
        let mut outcomes = Vec::new();
        for result in batch.results {
            let error = match result.outcome {
                ws::v2::SubscribeSetOutcome::Applied(rows) => {
                    tables.extend(rows.tables.into_vec().into_iter().map(|table| {
                        ws::v2::TableUpdate {
                            table_name: table.table,
                            rows: vec![ws::v2::TableUpdateRows::PersistentTable(ws::v2::PersistentTableRows {
                                inserts: table.rows,
                                deletes: Default::default(),
                            })]
                            .into(),
                        }
                    }));
                    None
                }
                ws::v2::SubscribeSetOutcome::Error(error) => Some(error.to_string()),
            };
            outcomes.push((result.query_set_id, error));
        }
        let update = M::DbUpdate::parse_update(ws::v2::TransactionUpdate {
            query_sets: vec![ws::v2::QuerySetUpdate {
                query_set_id: QuerySetId { id: 0 },
                tables: tables.into(),
            }]
            .into(),
        });
        let update = match update {
            Ok(update) => update,
            Err(error) => return self.connection_lost(Some(error), true, false),
        };
        self.apply_update(update, |inner| {
            for (id, error) in outcomes {
                if let Some(error) = error {
                    inner.subscriptions.subscription_error(
                        &self.make_event_ctx(Some(crate::Error::SubscriptionError { error })),
                        id,
                    );
                } else {
                    inner.subscriptions.subscription_applied(&self.make_event_ctx(()), id);
                }
            }
            Event::SubscribeApplied
        });
        Ok(())
    }

    fn fail_pending_operations(&self, inner: &mut DbContextImplInner<M>) {
        let error = InternalError::new("Connection lost before the result was received")
            .with_cause(crate::Error::UnknownResult);
        for (_, (reducer, callback)) in std::mem::take(&mut inner.reducer_callbacks.callbacks) {
            let ctx = self.make_event_ctx(ReducerEvent {
                reducer,
                timestamp: Timestamp::UNIX_EPOCH,
                status: Status::Panic(error.clone()),
            });
            callback(&ctx, Err(error.clone()));
        }
        for (_, callback) in std::mem::take(&mut inner.procedure_callbacks.request_id_to_callback) {
            callback(&self.make_event_ctx(()), Err(error.clone()));
        }
    }

    fn connection_lost(&self, error: Option<crate::Error>, terminal: bool, auth: bool) -> crate::Result<()> {
        let lifecycle = self.inner.lock().unwrap().connection_lifecycle;
        if lifecycle == ConnectionLifecycle::Ended {
            return Err(crate::Error::Disconnected);
        }
        *self.send_chan.lock().unwrap() = None;
        let next = {
            let mut state = self.reconnect.lock().unwrap();
            let can_retry = state.options.is_some()
                && self.identity.lock().unwrap().is_some()
                && !terminal
                && !(auth && (state.provider.is_none() || state.used_fresh_token));
            if can_retry {
                state.force_refresh |= auth;
                state.reconnecting = true;
                state.preparing_replay = true;
                state.attempt = state.attempt.saturating_add(1);
                Some(NextReconnect {
                    attempt: state.attempt,
                    delay: state.options.unwrap().delay(state.attempt, rand::random()),
                })
            } else {
                state.reconnecting = false;
                state.preparing_replay = false;
                state.generation += 1;
                if let Some(abort) = state.abort.take() {
                    abort.abort();
                }
                None
            }
        };
        if let Some(next) = next {
            self.schedule_reconnect(next);
        }
        let mut inner = self.inner.lock().unwrap();
        inner.connection_lifecycle = if next.is_some() {
            ConnectionLifecycle::Connecting
        } else {
            ConnectionLifecycle::Ended
        };
        inner.replay = None;
        self.fail_pending_operations(&mut inner);
        if next.is_some() {
            inner.subscriptions.suspend(&self.make_event_ctx(()));
        } else {
            inner.subscriptions.on_disconnect(&self.make_event_ctx(()));
        }
        let callback_error = if lifecycle == ConnectionLifecycle::Connecting {
            Some(error.unwrap_or_else(|| InternalError::new("Connection closed before InitialConnection").into()))
        } else {
            error
        };
        let ctx = self.make_event_ctx(callback_error.clone());
        if lifecycle == ConnectionLifecycle::Connected {
            if let Some(callback) = inner.on_disconnect.as_mut() {
                callback(&ctx, callback_error.clone(), next);
            }
        } else if let Some(callback) = inner.on_connect_error.as_mut() {
            callback(&ctx, callback_error.clone().unwrap(), next);
        }
        if next.is_some() {
            Ok(())
        } else {
            Err(callback_error.unwrap_or(crate::Error::Disconnected))
        }
    }

    fn schedule_reconnect(&self, next: NextReconnect) {
        let (park_send, park_recv) = mpsc::unbounded();
        *get_lock_sync(&self.recv) = park_recv;
        let mut state = self.reconnect.lock().unwrap();
        state.generation += 1;
        let generation = state.generation;
        let config = state.config.clone();
        let mut token = state.token.clone();
        let provider = state.provider.clone();
        let force_refresh = state.force_refresh;
        let (abort, registration) = AbortHandle::new_pair();
        state.abort = Some(abort);
        drop(state);
        let pending = self.pending_mutations_send.clone();
        let extra_logging = self.extra_logging.clone();
        #[cfg(not(feature = "browser"))]
        let runtime = self.runtime.clone();
        let task = async move {
            #[cfg(not(feature = "browser"))]
            tokio::time::sleep(next.delay).await;
            #[cfg(feature = "browser")]
            gloo_timers::future::TimeoutFuture::new(next.delay.as_millis().min(u32::MAX as u128) as u32).await;
            let provider = provider.filter(|_| {
                force_refresh || crate::reconnect::token_needs_refresh(token.as_deref(), crate::reconnect::now())
            });
            let fresh = provider.is_some();
            let result = async {
                if let Some(provider) = provider {
                    token = Some(provider.token().await.map_err(ReconnectFailure::Provider)?);
                    if token.as_ref().is_none_or(|token| token.is_empty()) {
                        return Err(ReconnectFailure::Provider(
                            InternalError::new("Token provider returned an empty token").into(),
                        ));
                    }
                }
                let socket =
                    WsConnection::connect(config.uri, &config.database_name, token.as_deref(), None, config.params)
                        .await
                        .map_err(ReconnectFailure::Transport)?;
                #[cfg(not(feature = "browser"))]
                let (_, raw_recv, send) = socket.spawn_message_loop(&runtime, extra_logging.clone());
                #[cfg(feature = "browser")]
                let (raw_recv, send) = socket.spawn_message_loop();
                #[cfg(not(feature = "browser"))]
                let (_, recv) = spawn_parse_loop(raw_recv, &runtime, extra_logging);
                #[cfg(feature = "browser")]
                let recv = spawn_parse_loop(raw_recv, extra_logging);
                Ok((send, recv))
            }
            .await;
            let _ = pending.unbounded_send(PendingMutation::ReconnectReady {
                _park_send: park_send,
                generation,
                token,
                fresh,
                result,
            });
        };
        #[cfg(not(feature = "browser"))]
        self.runtime.spawn(async move {
            let _ = Abortable::new(task, registration).await;
        });
        #[cfg(feature = "browser")]
        wasm_bindgen_futures::spawn_local(async move {
            let _ = Abortable::new(task, registration).await;
        });
    }

    fn make_event_ctx<E, Ctx: AbstractEventContext<Module = M, Event = E>>(&self, event: E) -> Ctx {
        let imp = self.clone();
        Ctx::new(imp, event)
    }

    /// Apply all queued [`PendingMutation`]s.
    fn apply_pending_mutations(&self) -> crate::Result<()> {
        while let Ok(Some(pending_mutation)) = get_lock_sync(&self.pending_mutations_recv).try_next() {
            self.apply_mutation(pending_mutation)?;
        }

        Ok(())
    }

    /// Apply an individual [`PendingMutation`].
    fn apply_mutation(&self, mutation: PendingMutation<M>) -> crate::Result<()> {
        let explicit_disconnect = matches!(&mutation, PendingMutation::Disconnect);
        match self.apply_mutation_inner(mutation) {
            Err(crate::Error::Disconnected) if !explicit_disconnect => self.connection_lost(None, false, false),
            result => result,
        }
    }

    fn apply_mutation_inner(&self, mutation: PendingMutation<M>) -> crate::Result<()> {
        self.debug_log(|out| writeln!(out, "`apply_mutation`: {mutation:?}"));
        match mutation {
            PendingMutation::ReconnectReady {
                _park_send,
                generation,
                token,
                fresh,
                result,
            } => {
                {
                    let mut state = self.reconnect.lock().unwrap();
                    if generation != state.generation {
                        return Ok(());
                    }
                    state.used_fresh_token = fresh;
                    if fresh && !matches!(&result, Err(ReconnectFailure::Provider(_))) {
                        state.token = token;
                        state.force_refresh = false;
                    }
                }
                match result {
                    Ok((send, recv)) => {
                        *self.send_chan.lock().unwrap() = Some(send);
                        *get_lock_sync(&self.recv) = recv;
                    }
                    Err(ReconnectFailure::Provider(error)) => return self.connection_lost(Some(error), false, false),
                    Err(ReconnectFailure::Transport(error)) => {
                        let terminal = error.is_terminal();
                        let auth = error.is_auth();
                        return self.connection_lost(
                            Some(InternalError::new(error.to_string()).with_cause(error).into()),
                            terminal,
                            auth,
                        );
                    }
                }
            }

            // Subscribe: register the subscription in the [`SubscriptionManager`]
            // and send the `Subscribe` WS message.
            PendingMutation::Subscribe { query_set_id, handle } => {
                let mut inner = self.inner.lock().unwrap();
                // Register the subscription, so we can handle related messages from the server.
                inner.subscriptions.register_subscription(query_set_id, handle.clone());
                if self.reconnect.lock().unwrap().preparing_replay {
                    return Ok(());
                }
                if let Some(msg) = handle.start() {
                    self.send_chan
                        .lock()
                        .unwrap()
                        .as_mut()
                        .ok_or(crate::Error::Disconnected)?
                        .unbounded_send(ws::v2::ClientMessage::Subscribe(msg))
                        .map_err(|_| crate::Error::Disconnected)?;
                }
                // else, the handle was already cancelled.
            }

            PendingMutation::Unsubscribe { query_set_id } => {
                let mut inner = self.inner.lock().unwrap();
                if self.reconnect.lock().unwrap().preparing_replay {
                    inner
                        .subscriptions
                        .unsubscribe_applied(&self.make_event_ctx(()), query_set_id);
                    return Ok(());
                }
                match inner.subscriptions.handle_pending_unsubscribe(query_set_id) {
                    PendingUnsubscribeResult::DoNothing =>
                    // The subscription was already unsubscribed, so we don't need to send an unsubscribe message.
                    {
                        return Ok(())
                    }

                    PendingUnsubscribeResult::RunCallback(callback) => {
                        callback(&self.make_event_ctx(()));
                    }
                    PendingUnsubscribeResult::SendUnsubscribe(m) => {
                        self.send_chan
                            .lock()
                            .unwrap()
                            .as_mut()
                            .ok_or(crate::Error::Disconnected)?
                            .unbounded_send(ws::v2::ClientMessage::Unsubscribe(m))
                            .map_err(|_| crate::Error::Disconnected)?;
                    }
                }
            }

            // CallReducer: send the `CallReducer` WS message.
            PendingMutation::InvokeReducerWithCallback { reducer, callback } => {
                if !self.is_active() {
                    let error = InternalError::new("Disconnected").with_cause(crate::Error::Disconnected);
                    callback(
                        &self.make_event_ctx(ReducerEvent {
                            reducer,
                            timestamp: Timestamp::UNIX_EPOCH,
                            status: Status::Panic(error.clone()),
                        }),
                        Err(error),
                    );
                    return Ok(());
                }
                let request_id = next_request_id();

                let reducer_name = reducer.reducer_name();
                let args = reducer
                    .args_bsatn()
                    .map_err(|e| InternalError::new("Failed to BSATN-serialize reducer arguments").with_cause(e))?;

                self.inner
                    .lock()
                    .unwrap()
                    .reducer_callbacks
                    .store_call_info(request_id, reducer, callback);

                let flags = ws::v2::CallReducerFlags::Default;
                let msg = ws::v2::ClientMessage::CallReducer(ws::v2::CallReducer {
                    reducer: reducer_name.into(),
                    args: args.into(),
                    request_id,
                    flags,
                });
                self.send_chan
                    .lock()
                    .unwrap()
                    .as_mut()
                    .ok_or(crate::Error::Disconnected)?
                    .unbounded_send(msg)
                    .map_err(|_| crate::Error::Disconnected)?;
            }

            // Invoke a procedure: stash its callback, then send the `CallProcedure` WS message.
            PendingMutation::InvokeProcedureWithCallback {
                procedure,
                args,
                callback,
            } => {
                if !self.is_active() {
                    callback(
                        &self.make_event_ctx(()),
                        Err(InternalError::new("Disconnected").with_cause(crate::Error::Disconnected)),
                    );
                    return Ok(());
                }
                // We need to include a request_id in the message so that we can find the callback once it completes.
                let request_id = next_request_id();
                self.inner
                    .lock()
                    .unwrap()
                    .procedure_callbacks
                    .insert(request_id, callback);

                let msg = ws::v2::ClientMessage::CallProcedure(ws::v2::CallProcedure {
                    procedure: procedure.into(),
                    args: args.into(),
                    request_id,
                    flags: ws::v2::CallProcedureFlags::Default,
                });
                self.send_chan
                    .lock()
                    .unwrap()
                    .as_mut()
                    .ok_or(crate::Error::Disconnected)?
                    .unbounded_send(msg)
                    .map_err(|_| crate::Error::Disconnected)?;
            }

            // Disconnect: close the connection.
            PendingMutation::Disconnect => {
                {
                    let mut state = self.reconnect.lock().unwrap();
                    state.generation += 1;
                    state.reconnecting = false;
                    state.preparing_replay = false;
                    if let Some(abort) = state.abort.take() {
                        abort.abort();
                    }
                }
                *self.send_chan.lock().unwrap() = None;
                let mut inner = self.inner.lock().unwrap();
                inner.connection_lifecycle = ConnectionLifecycle::Ended;
                inner.replay = None;
                self.fail_pending_operations(&mut inner);
                let ctx = self.make_event_ctx(None);
                inner.subscriptions.on_disconnect(&self.make_event_ctx(()));
                if let Some(callback) = inner.on_disconnect.as_mut() {
                    callback(&ctx, None, None);
                }
                return Err(crate::Error::Disconnected);
            }

            // Callback stuff: these all do what you expect.
            PendingMutation::AddInsertCallback {
                table,
                callback_id,
                callback,
            } => {
                self.inner
                    .lock()
                    .unwrap()
                    .db_callbacks
                    .get_table_callbacks(table)
                    .register_on_insert(callback_id, callback);
            }
            PendingMutation::AddDeleteCallback {
                table,
                callback_id,
                callback,
            } => {
                self.inner
                    .lock()
                    .unwrap()
                    .db_callbacks
                    .get_table_callbacks(table)
                    .register_on_delete(callback_id, callback);
            }
            PendingMutation::AddUpdateCallback {
                table,
                callback_id,
                callback,
            } => {
                self.inner
                    .lock()
                    .unwrap()
                    .db_callbacks
                    .get_table_callbacks(table)
                    .register_on_update(callback_id, callback);
            }
            PendingMutation::RemoveInsertCallback { table, callback_id } => {
                self.inner
                    .lock()
                    .unwrap()
                    .db_callbacks
                    .get_table_callbacks(table)
                    .remove_on_insert(callback_id);
            }
            PendingMutation::RemoveDeleteCallback { table, callback_id } => {
                self.inner
                    .lock()
                    .unwrap()
                    .db_callbacks
                    .get_table_callbacks(table)
                    .remove_on_delete(callback_id);
            }
            PendingMutation::RemoveUpdateCallback { table, callback_id } => {
                self.inner
                    .lock()
                    .unwrap()
                    .db_callbacks
                    .get_table_callbacks(table)
                    .remove_on_update(callback_id);
            }
        };
        Ok(())
    }

    /// If a WebSocket message is waiting, process it and return `true`.
    /// If no WebSocket messages are in the queue, immediately return `false`.
    ///
    /// Called by the autogenerated `DbConnection` method of the same name.
    pub fn advance_one_message(&self) -> crate::Result<bool> {
        // Apply any pending mutations before processing a WS message,
        // so that pending callbacks don't get skipped.
        self.apply_pending_mutations()?;

        // Deranged behavior: mpsc's `try_next` returns `Ok(None)` when the channel is closed,
        // and `Err(_)` when the channel is open and waiting. This seems exactly backwards.
        //
        // NOTE(cloutiertyler): A comment on the deranged behavior: the mental
        // model is that of an iterator, but for a stream instead. i.e. you pull
        // off of an iterator until it returns `None`, which means that the
        // iterator is exhausted. If you try to pull off the iterator and
        // there's nothing there but it's not exhausted, it (arguably sensibly)
        // returns `Err(_)`. Similar behavior as `Iterator::next` and
        // `Stream::poll_next`. No comment on whether this is a good mental
        // model or not.
        let incoming = get_lock_sync(&self.recv).try_next();
        let res = match incoming {
            Ok(None) => self.connection_lost(None, false, false).map(|_| true),
            Err(_) => Ok(false),
            Ok(Some(msg)) => self.process_message(msg).map(|_| true),
        };

        // Also apply any new pending messages afterwards,
        // so that outgoing WS messages get sent as soon as possible.
        self.apply_pending_mutations()?;

        res
    }

    async fn get_message(&self) -> Message<M> {
        // Holding these locks across the below await can only cause a deadlock if
        // there are multiple parallel callers of `advance_one_message` or its siblings.
        // We call this out as an incorrect and unsupported thing to do.
        #![allow(clippy::await_holding_lock)]

        let mut pending_mutations = get_lock_async(&self.pending_mutations_recv).await;
        let mut recv = get_lock_async(&self.recv).await;

        // Always process pending mutations before WS messages, if they're available,
        // so that newly registered callbacks run on messages.
        // This may be unnecessary, but `tokio::select` does not document any ordering guarantees,
        // and if both `pending_mutations.next()` and `recv.next()` have values ready,
        // we want to process the pending mutation first.
        if let Ok(pending_mutation) = pending_mutations.try_next() {
            return Message::Local(pending_mutation.unwrap());
        }

        #[cfg(not(feature = "browser"))]
        tokio::select! {
            pending_mutation = pending_mutations.next() => Message::Local(pending_mutation.unwrap()),
            incoming_message = recv.next() => Message::Ws(incoming_message),
        }

        #[cfg(feature = "browser")]
        {
            let (pending_fut, recv_fut) = (pending_mutations.next().fuse(), recv.next().fuse());
            pin_mut!(pending_fut, recv_fut);

            futures::select! {
                pending_mutation = pending_fut => Message::Local(pending_mutation.unwrap()),
                incoming_message = recv_fut => Message::Ws(incoming_message),
            }
        }
    }

    /// Like [`Self::advance_one_message`], but sleeps the thread until a message is available.
    ///
    /// Called by the autogenerated `DbConnection` method of the same name.
    #[cfg(not(feature = "browser"))]
    pub fn advance_one_message_blocking(&self) -> crate::Result<()> {
        match self.runtime.block_on(self.get_message()) {
            Message::Local(pending) => self.apply_mutation(pending),
            Message::Ws(None) => self.connection_lost(None, false, false),
            Message::Ws(Some(msg)) => self.process_message(msg),
        }
    }

    /// Like [`Self::advance_one_message`], but `await`s until a message is available.
    ///
    /// Called by the autogenerated `DbConnection` method of the same name.
    pub async fn advance_one_message_async(&self) -> crate::Result<()> {
        match self.get_message().await {
            Message::Local(pending) => self.apply_mutation(pending),
            Message::Ws(None) => self.connection_lost(None, false, false),
            Message::Ws(Some(msg)) => self.process_message(msg),
        }
    }

    /// Call [`Self::advance_one_message`] in a loop until no more messages are waiting.
    ///
    /// Called by the autogenerated `DbConnection` method of the same name.
    pub fn frame_tick(&self) -> crate::Result<()> {
        while self.advance_one_message()? {}
        Ok(())
    }

    /// Spawn a thread which does [`Self::advance_one_message_blocking`] in a loop.
    ///
    /// Called by the autogenerated `DbConnection` method of the same name.
    #[cfg(not(feature = "browser"))]
    pub fn run_threaded(&self) -> std::thread::JoinHandle<()> {
        let this = self.clone();
        std::thread::spawn(move || loop {
            match this.advance_one_message_blocking() {
                Ok(()) => (),
                Err(e) if error_is_normal_disconnect(&e) => return,
                Err(e) => panic!("{e:?}"),
            }
        })
    }

    /// Spawn a background task which does [`Self::advance_one_message_async`] in a loop.
    ///
    /// Called by the autogenerated `DbConnection` method of the same name.
    #[cfg(feature = "browser")]
    pub fn run_background_task(&self) {
        let this = self.clone();
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                match this.advance_one_message_async().await {
                    Ok(()) => (),
                    Err(e) if error_is_normal_disconnect(&e) => return,
                    Err(e) => panic!("{e:?}"),
                }
            }
        })
    }

    /// An async task which does [`Self::advance_one_message_async`] in a loop.
    ///
    /// Called by the autogenerated `DbConnection` method of the same name.
    pub async fn run_async(&self) -> crate::Result<()> {
        let this = self.clone();
        loop {
            match this.advance_one_message_async().await {
                Ok(()) => (),
                Err(e) if error_is_normal_disconnect(&e) => return Ok(()),
                Err(e) => return Err(e),
            }
        }
    }

    /// Called by the autogenerated `DbConnection` method of the same name.
    pub fn is_active(&self) -> bool {
        !self.is_reconnecting()
            && self
                .send_chan
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|sender| !sender.is_closed())
    }

    /// True while waiting for or establishing an automatic reconnect.
    pub fn is_reconnecting(&self) -> bool {
        self.reconnect.lock().unwrap().reconnecting
    }

    /// Called by the autogenerated `DbConnection` method of the same name.
    pub fn disconnect(&self) -> crate::Result<()> {
        if !self.is_active() && !self.is_reconnecting() {
            return Err(crate::Error::Disconnected);
        }
        self.pending_mutations_send
            .unbounded_send(PendingMutation::Disconnect)
            .unwrap();
        Ok(())
    }

    /// Add a [`PendingMutation`] to the `pending_mutations` queue,
    /// to be processed during the next call to [`Self::apply_pending_mutations`].
    ///
    /// This is used to defer operations which would otherwise need to hold a lock on `self.inner`,
    /// as otherwise running those operations within a callback would deadlock.
    fn queue_mutation(&self, mutation: PendingMutation<M>) {
        self.pending_mutations_send.unbounded_send(mutation).unwrap();
    }

    /// Called by autogenerated table access methods.
    pub fn get_table<Row: InModule<Module = M> + Send + Sync + 'static>(
        &self,
        table_name: &'static str,
    ) -> TableHandle<Row> {
        let client_cache = Arc::clone(&self.cache);
        let pending_mutations = self.pending_mutations_send.clone();
        TableHandle {
            client_cache,
            pending_mutations,
            table_name,
        }
    }

    /// Called by autogenerated reducer invocation methods.
    pub fn invoke_reducer_with_callback<Args>(
        &self,
        reducer: Args,
        callback: impl FnOnce(&<M as SpacetimeModule>::ReducerEventContext, Result<Result<(), String>, InternalError>)
            + Send
            + 'static,
    ) -> crate::Result<()>
    where
        <M as SpacetimeModule>::Reducer: From<Args>,
    {
        if !self.is_active() {
            return Err(crate::Error::Disconnected);
        }
        self.queue_mutation(PendingMutation::InvokeReducerWithCallback {
            reducer: reducer.into(),
            callback: Box::new(callback),
        });
        Ok(())
    }

    /// Called by the autogenerated `DbConnection` method of the same name.
    pub fn try_identity(&self) -> Option<Identity> {
        *self.identity.lock().unwrap()
    }

    /// Called by the autogenerated `DbConnection` method of the same name.
    /// TODO: Deprecate and add a `try_identity`.
    pub fn connection_id(&self) -> ConnectionId {
        self.try_connection_id().unwrap()
    }

    /// Called by the autogenerated `DbConnection` method of the same name.
    pub fn try_connection_id(&self) -> Option<ConnectionId> {
        *self.connection_id.lock().unwrap()
    }

    pub fn invoke_procedure_with_callback<
        Args: Serialize + InModule<Module = M>,
        RetVal: for<'a> Deserialize<'a> + 'static,
    >(
        &self,
        procedure_name: &'static str,
        args: Args,
        callback: impl FnOnce(&<M as SpacetimeModule>::ProcedureEventContext, Result<RetVal, InternalError>)
            + Send
            + 'static,
    ) {
        if !self.is_active() {
            callback(
                &self.make_event_ctx(()),
                Err(InternalError::new("Disconnected").with_cause(crate::Error::Disconnected)),
            );
            return;
        }
        self.queue_mutation(PendingMutation::InvokeProcedureWithCallback {
            procedure: procedure_name,
            args: bsatn::to_vec(&args).expect("Failed to BSATN serialize procedure args"),
            callback: Box::new(move |ctx, ret| {
                callback(
                    ctx,
                    ret.map(|ret| {
                        bsatn::from_slice::<RetVal>(&ret[..])
                            .expect("Failed to BSATN deserialize procedure return value")
                    }),
                )
            }),
        });
    }
}

type OnConnectCallback<M> = Box<dyn FnOnce(&<M as SpacetimeModule>::DbConnection, Identity, &str) + Send + 'static>;

type OnAutomaticReconnectCallback<M> =
    Box<dyn FnMut(&<M as SpacetimeModule>::DbConnection, Identity, &str) + Send + 'static>;

type OnConnectErrorCallback<M> =
    Box<dyn FnMut(&<M as SpacetimeModule>::ErrorContext, crate::Error, Option<NextReconnect>) + Send + 'static>;

type OnDisconnectCallback<M> =
    Box<dyn FnMut(&<M as SpacetimeModule>::ErrorContext, Option<crate::Error>, Option<NextReconnect>) + Send + 'static>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionLifecycle {
    /// Waiting for the server's initial connection message.
    Connecting,
    /// The server has sent the initial connection message.
    Connected,
    /// The connection has already reached a terminal lifecycle state.
    Ended,
}

/// All the stuff in a [`DbContextImpl`] which can safely be locked while invoking callbacks.
pub(crate) struct DbContextImplInner<M: SpacetimeModule> {
    /// `Some` if not within the context of an outer runtime. The `Runtime` must
    /// then live as long as `Self`.
    #[allow(unused)]
    #[cfg(not(feature = "browser"))]
    runtime: Option<Runtime>,

    db_callbacks: DbCallbacks<M>,
    reducer_callbacks: ReducerCallbacks<M>,
    pub(crate) subscriptions: SubscriptionManager<M>,

    connection_lifecycle: ConnectionLifecycle,
    on_connect: Option<OnConnectCallback<M>>,
    on_automatic_reconnect: Option<OnAutomaticReconnectCallback<M>>,
    on_connect_error: Option<OnConnectErrorCallback<M>>,
    on_disconnect: Option<OnDisconnectCallback<M>>,

    procedure_callbacks: ProcedureCallbacks<M>,
    replay: Option<(u32, Vec<QuerySetId>)>,
}

/// A builder-pattern constructor for a `DbConnection` connection to the module `M`.
///
/// `M` will be the autogenerated opaque module type.
///
/// Get a builder by calling `DbConnection::builder()`.
// TODO: Move into its own module which is not #[doc(hidden)]?
pub struct DbConnectionBuilder<M: SpacetimeModule> {
    uri: Option<Uri>,

    database_name: Option<String>,

    token: Option<String>,

    on_connect: Option<OnConnectCallback<M>>,
    on_automatic_reconnect: Option<OnAutomaticReconnectCallback<M>>,
    on_connect_error: Option<OnConnectErrorCallback<M>>,
    on_disconnect: Option<OnDisconnectCallback<M>>,

    additional_logging_path: Option<PathBuf>,

    params: WsParams,
    reconnect_options: Option<AutomaticReconnectOptions>,
    token_provider: Option<Arc<dyn TokenProvider>>,
}

/// This process's global connection ID, which will be attacked to all connections it makes.
// TODO: rip this out. Make the connection id a property of the `DbConnection`. Cloud can supply it to the builder.
static CONNECTION_ID: OnceLock<ConnectionId> = OnceLock::new();

fn get_connection_id_override() -> Option<ConnectionId> {
    CONNECTION_ID.get().copied()
}

#[doc(hidden)]
/// Attempt to set this process's connection ID to a known value.
///
/// This functionality is exposed for use in SpacetimeDB-cloud.
/// It is unstable, and will be removed without warning in a future version.
///
/// Clients which want a particular connection ID must call this method
/// before constructing any connection.
/// Once any connection is constructed, the per-process connection ID value is locked in,
/// and cannot be overwritten.
///
/// Returns `Err` if this process's connection ID has already been initialized to a random value.
pub fn set_connection_id(id: ConnectionId) -> crate::Result<()> {
    let stored = *CONNECTION_ID.get_or_init(|| id);

    if stored != id {
        return Err(InternalError::new(
            "Call to set_connection_id after CONNECTION_ID was initialized to a different value ",
        )
        .into());
    }
    Ok(())
}

pub(crate) fn debug_log(
    extra_logging: &Option<SharedCell<File>>,
    body: impl FnOnce(&mut File) -> std::result::Result<(), std::io::Error>,
) {
    if let Some(file) = extra_logging {
        body(&mut file.lock().expect("`extra_logging` file Mutex is poisoned")).expect("Writing debug log failed")
    }
}

impl<M: SpacetimeModule> DbConnectionBuilder<M> {
    /// Implementation of the generated `DbConnection::builder` method.
    /// Call that method instead.
    #[doc(hidden)]
    pub fn new() -> Self {
        Self {
            uri: None,
            database_name: None,
            token: None,
            on_connect: None,
            on_automatic_reconnect: None,
            on_connect_error: None,
            on_disconnect: None,
            additional_logging_path: None,
            params: <_>::default(),
            reconnect_options: None,
            token_provider: None,
        }
    }

    /// Enable unlimited automatic reconnect with the default backoff bounds.
    pub fn with_automatic_reconnect(self) -> Self {
        self.with_automatic_reconnect_options(AutomaticReconnectOptions::default())
    }

    /// Enable automatic reconnect using custom backoff bounds.
    pub fn with_automatic_reconnect_options(mut self, options: AutomaticReconnectOptions) -> Self {
        self.reconnect_options = Some(options.resolve());
        self
    }

    pub fn with_token_provider(mut self, provider: impl TokenProvider) -> Self {
        self.token_provider = Some(Arc::new(provider));
        self
    }

    /// Called for each successful automatic reconnect, before subscription replay.
    /// `on_connect` is reserved for the initial connection.
    pub fn on_automatic_reconnect(
        mut self,
        callback: impl FnMut(&M::DbConnection, Identity, &str) + Send + 'static,
    ) -> Self {
        self.on_automatic_reconnect = Some(Box::new(callback));
        self
    }

    fn reconnect_state(&mut self) -> ReconnectState {
        if self.reconnect_options.is_some() {
            self.params.session_id = Some(ConnectionId::from_u128(rand::random()));
        }
        ReconnectState {
            config: ConnectionConfig {
                uri: self.uri.clone().expect("with_uri is required"),
                database_name: self.database_name.clone().expect("with_database_name is required"),
                params: self.params,
            },
            options: self.reconnect_options,
            provider: self.token_provider.clone(),
            token: self.token.clone(),
            attempt: 0,
            generation: 0,
            reconnecting: false,
            preparing_replay: false,
            used_fresh_token: false,
            force_refresh: false,
            abort: None,
        }
    }

    /// Open a WebSocket connection to the remote database,
    /// with all configuration and callbacks registered in the builder `self`.
    ///
    /// This method panics if `self` lacks a required configuration,
    /// or returns an `Err` if some I/O operation during the initial WebSocket connection fails.
    ///
    /// Successful return from this method does not necessarily imply a valid `DbConnection`;
    /// the connection may still fail asynchronously,
    /// leading to the [`Self::on_connect_error`] callback being invoked.
    ///
    /// Before calling this method, make sure to invoke at least [`Self::with_uri`] and [`Self::with_database_name`]
    /// to configure the connection.
    #[must_use = "
You must explicitly advance the connection by calling any one of:

- `DbConnection::frame_tick`.
- `DbConnection::run_threaded`.
- `DbConnection::run_background_task`.
- `DbConnection::run_async`.
- `DbConnection::advance_one_message`.
- `DbConnection::advance_one_message_blocking`.
- `DbConnection::advance_one_message_async`.

Which of these methods you should call depends on the specific needs of your application,
but you must call one of them, or else the connection will never progress.
"]
    #[cfg(not(feature = "browser"))]
    pub fn build(self) -> crate::Result<M::DbConnection> {
        let imp = self.build_impl()?;
        Ok(<M::DbConnection as DbConnection>::new(imp))
    }

    #[cfg(feature = "browser")]
    pub async fn build(self) -> crate::Result<M::DbConnection> {
        let imp = self.build_impl().await?;
        Ok(<M::DbConnection as DbConnection>::new(imp))
    }

    /// Open a WebSocket connection, build an empty client cache, &c,
    /// to construct a [`DbContextImpl`].
    #[cfg(not(feature = "browser"))]
    fn build_impl(mut self) -> crate::Result<DbContextImpl<M>> {
        let reconnect = self.reconnect_state();
        let extra_logging = self
            .additional_logging_path
            .map(|path| {
                OpenOptions::new().append(true).create(true).open(&path).map_err(|e| {
                    InternalError::new(format!("Failed to open file '{path:?}' for additional logging")).with_cause(e)
                })
            })
            .transpose()?
            .map(|file| Arc::new(StdMutex::new(file)));

        let (runtime, handle) = enter_or_create_runtime()?;

        let connection_id_override = get_connection_id_override();
        let ws_connection = tokio::task::block_in_place(|| {
            handle.block_on(WsConnection::connect(
                self.uri.clone().unwrap(),
                self.database_name.as_ref().unwrap(),
                self.token.as_deref(),
                connection_id_override,
                self.params,
            ))
        })
        .map_err(|source| crate::Error::FailedToConnect {
            source: InternalError::new("Failed to initiate WebSocket connection").with_cause(source),
        })?;

        let (_websocket_loop_handle, raw_msg_recv, raw_msg_send) =
            ws_connection.spawn_message_loop(&handle, extra_logging.clone());
        let (_parse_loop_handle, parsed_recv_chan) =
            spawn_parse_loop::<M>(raw_msg_recv, &handle, extra_logging.clone());
        let parsed_recv_chan = Arc::new(TokioMutex::new(parsed_recv_chan));

        let (pending_mutations_send, pending_mutations_recv) = mpsc::unbounded();
        let pending_mutations_recv = Arc::new(TokioMutex::new(pending_mutations_recv));

        let inner_ctx = build_db_ctx_inner(
            runtime,
            self.on_connect,
            self.on_automatic_reconnect,
            self.on_connect_error,
            self.on_disconnect,
        );
        Ok(build_db_ctx(
            handle,
            inner_ctx,
            raw_msg_send,
            parsed_recv_chan,
            pending_mutations_send,
            pending_mutations_recv,
            connection_id_override,
            extra_logging,
            reconnect,
        ))
    }

    /// Open a WebSocket connection, build an empty client cache, &c,
    /// to construct a [`DbContextImpl`].
    #[cfg(feature = "browser")]
    async fn build_impl(mut self) -> crate::Result<DbContextImpl<M>> {
        let reconnect = self.reconnect_state();
        // The wasm/browser SDK target runs under `wasm32-unknown-unknown`, where we do not
        // have the native file APIs that back `with_debug_to_file`. Keeping the
        // shared `extra_logging` field as `None` lets the rest of the connection and
        // cache code stay unified without pretending that file logging works in browser.
        //
        // TODO: Make this work in browser targets by logging to the browser console.
        let extra_logging = None;
        let connection_id_override = get_connection_id_override();
        let ws_connection = WsConnection::connect(
            self.uri.clone().unwrap(),
            self.database_name.as_ref().unwrap(),
            self.token.as_deref(),
            connection_id_override,
            self.params,
        )
        .await
        .map_err(|source| crate::Error::FailedToConnect {
            source: InternalError::new("Failed to initiate WebSocket connection").with_cause(source),
        })?;

        let (raw_msg_recv, raw_msg_send) = ws_connection.spawn_message_loop();
        let parsed_recv_chan = spawn_parse_loop::<M>(raw_msg_recv, extra_logging.clone());
        let parsed_recv_chan = Arc::new(StdMutex::new(parsed_recv_chan));

        let (pending_mutations_send, pending_mutations_recv) = mpsc::unbounded();
        let pending_mutations_recv = Arc::new(StdMutex::new(pending_mutations_recv));

        let inner_ctx = build_db_ctx_inner(
            self.on_connect,
            self.on_automatic_reconnect,
            self.on_connect_error,
            self.on_disconnect,
        );
        Ok(build_db_ctx(
            inner_ctx,
            raw_msg_send,
            parsed_recv_chan,
            pending_mutations_send,
            pending_mutations_recv,
            connection_id_override,
            extra_logging,
            reconnect,
        ))
    }

    /// Set the URI of the SpacetimeDB host which is running the remote database.
    ///
    /// The URI must have either no scheme or one of the schemes `http`, `https`, `ws` or `wss`.
    pub fn with_uri<E: std::fmt::Debug>(mut self, uri: impl TryInto<Uri, Error = E>) -> Self {
        let uri = uri.try_into().expect("Unable to parse supplied URI");
        self.uri = Some(uri);
        self
    }

    /// Set the name or identity of the remote database to connect to.
    pub fn with_database_name(mut self, name_or_identity: impl Into<String>) -> Self {
        self.database_name = Some(name_or_identity.into());
        self
    }

    /// Supply a token with which to authenticate with the remote database.
    ///
    /// `token` should be an OpenID Connect compliant JSON Web Token.
    ///
    /// If this method is not invoked, or `None` is supplied,
    /// the SpacetimeDB host will generate a new anonymous `Identity`.
    ///
    /// If the token is rejected before a connection context is created, [`Self::build`]
    /// returns an error. If the host reports the rejection after the WebSocket is
    /// established but before the initial connection message, [`Self::on_connect_error`]
    /// is invoked.
    pub fn with_token(mut self, token: Option<impl Into<String>>) -> Self {
        self.token = token.map(|token| token.into());
        self
    }

    /// Sets the compression used when a certain threshold in the message size has been reached.
    ///
    /// The current threshold used by the host is 1KiB for the entire server message
    /// and for individual query updates.
    /// Note however that this threshold is not guaranteed and may change without notice.
    pub fn with_compression(mut self, compression: ws::common::Compression) -> Self {
        self.params.compression = compression;
        self
    }

    /// Sets whether to use confirmed reads.
    ///
    /// When enabled, the server will send query results only after they are
    /// confirmed to be durable.
    ///
    /// What durable means depends on the server configuration: a single node
    /// server may consider a transaction durable once it is `fsync`'ed to disk,
    /// a cluster after some number of replicas have acknowledged that they
    /// have stored the transaction.
    ///
    /// Note that enabling confirmed reads will increase the latency between a
    /// reducer call and the corresponding subscription update arriving at the
    /// client.
    ///
    /// If this method is not called, the server chooses the default.
    pub fn with_confirmed_reads(mut self, confirmed: bool) -> Self {
        self.params.confirmed = Some(confirmed);
        self
    }

    /// Set `path` as a path for additional debug logging related to SDK internals.
    ///
    /// When enabled, the SDK will create or open `path` for write-append and write logs to it.
    /// This is useful for diagnosing bugs in the SDK,
    /// but will generate a large volume of text logs and may have performance overhead,
    /// so it should not be used in production.
    ///
    /// When running multiple connections in parallel,
    /// either within the same process or from separate processes,
    /// prefer giving each its own unique path here;
    /// multiple `DbConnection`s writing to the same debug file concurrently
    /// may interleave or corrupt the output.
    pub fn with_debug_to_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.additional_logging_path = Some(path.into());
        self
    }

    /// Register a callback to run when the connection is successfully established.
    ///
    /// The connection is established after the initial connection message is
    /// received from the host. The callback will receive three arguments:
    /// - The `DbConnection` which has successfully connected.
    /// - The `Identity` of the successful connection.
    /// - The private access token which can be used to later re-authenticate as the same `Identity`.
    ///   If a token was passed to [`Self::with_token`],
    ///   this will be the same token.
    pub fn on_connect(mut self, callback: impl FnOnce(&M::DbConnection, Identity, &str) + Send + 'static) -> Self {
        if self.on_connect.is_some() {
            panic!(
                "DbConnectionBuilder can only register a single `on_connect` callback.

Instead of registering multiple `on_connect` callbacks, register a single callback which does multiple operations."
            );
        }

        self.on_connect = Some(Box::new(callback));
        self
    }

    /// Register a callback to run when a connection attempt fails asynchronously.
    ///
    /// This callback is invoked only before the initial connection message is
    /// received from the host. Errors which prevent [`Self::build`] from creating
    /// a connection are returned by [`Self::build`] instead.
    pub fn on_connect_error(
        mut self,
        callback: impl FnMut(&M::ErrorContext, crate::Error, Option<NextReconnect>) + Send + 'static,
    ) -> Self {
        if self.on_connect_error.is_some() {
            panic!(
                "DbConnectionBuilder can only register a single `on_connect_error` callback.

Instead of registering multiple `on_connect_error` callbacks, register a single callback which does multiple operations."
            );
        }

        self.on_connect_error = Some(Box::new(callback));
        self
    }

    /// Register a callback to run when an established connection is closed.
    ///
    /// The connection is established after the initial connection message is
    /// received from the host. Connection failures before that point invoke
    /// [`Self::on_connect_error`] instead.
    pub fn on_disconnect(
        mut self,
        callback: impl FnMut(&M::ErrorContext, Option<crate::Error>, Option<NextReconnect>) + Send + 'static,
    ) -> Self {
        if self.on_disconnect.is_some() {
            panic!(
                "DbConnectionBuilder can only register a single `on_disconnect` callback.

Instead of registering multiple `on_disconnect` callbacks, register a single callback which does multiple operations."
            );
        }
        self.on_disconnect = Some(Box::new(callback));
        self
    }
}

/// Create a [`DbContextImplInner`] wrapped in `Arc<Mutex<...>>`.
fn build_db_ctx_inner<M: SpacetimeModule>(
    #[cfg(not(feature = "browser"))] runtime: Option<Runtime>,

    on_connect_cb: Option<OnConnectCallback<M>>,
    on_automatic_reconnect: Option<OnAutomaticReconnectCallback<M>>,
    on_connect_error_cb: Option<OnConnectErrorCallback<M>>,
    on_disconnect_cb: Option<OnDisconnectCallback<M>>,
) -> Arc<StdMutex<DbContextImplInner<M>>> {
    Arc::new(StdMutex::new(DbContextImplInner {
        #[cfg(not(feature = "browser"))]
        runtime,

        db_callbacks: DbCallbacks::default(),
        reducer_callbacks: ReducerCallbacks::default(),
        subscriptions: SubscriptionManager::default(),

        connection_lifecycle: ConnectionLifecycle::Connecting,
        on_connect: on_connect_cb,
        on_automatic_reconnect,
        on_connect_error: on_connect_error_cb,
        on_disconnect: on_disconnect_cb,

        procedure_callbacks: ProcedureCallbacks::default(),
        replay: None,
    }))
}

#[allow(clippy::too_many_arguments)]
/// Assemble and return a [`DbContextImpl`] from the provided [`DbContextImplInner`], and channels.
fn build_db_ctx<M: SpacetimeModule>(
    #[cfg(not(feature = "browser"))] runtime_handle: runtime::Handle,

    inner_ctx: Arc<StdMutex<DbContextImplInner<M>>>,
    raw_msg_send: mpsc::UnboundedSender<ws::v2::ClientMessage>,
    parsed_msg_recv: SharedAsyncCell<mpsc::UnboundedReceiver<ParsedMessage<M>>>,
    pending_mutations_send: mpsc::UnboundedSender<PendingMutation<M>>,
    pending_mutations_recv: SharedAsyncCell<mpsc::UnboundedReceiver<PendingMutation<M>>>,
    connection_id: Option<ConnectionId>,
    extra_logging: Option<SharedCell<File>>,
    reconnect: ReconnectState,
) -> DbContextImpl<M> {
    let mut cache = ClientCache::new(extra_logging.clone());
    M::register_tables(&mut cache);
    let cache = Arc::new(StdMutex::new(cache));

    DbContextImpl {
        #[cfg(not(feature = "browser"))]
        runtime: runtime_handle,
        inner: inner_ctx,
        send_chan: Arc::new(StdMutex::new(Some(raw_msg_send))),
        cache,
        recv: parsed_msg_recv,
        pending_mutations_send,
        pending_mutations_recv,
        identity: Arc::new(StdMutex::new(None)),
        connection_id: Arc::new(StdMutex::new(connection_id)),
        extra_logging,
        reconnect: Arc::new(StdMutex::new(reconnect)),
    }
}

// When called from within an async context, return a handle to it (and no
// `Runtime`), otherwise create a fresh `Runtime` and return it along with a
// handle to it.
#[cfg(not(feature = "browser"))]
fn enter_or_create_runtime() -> crate::Result<(Option<Runtime>, runtime::Handle)> {
    match runtime::Handle::try_current() {
        Err(e) if e.is_missing_context() => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(1)
                .thread_name("spacetimedb-background-connection")
                .build()
                .map_err(|source| InternalError::new("Failed to create Tokio runtime").with_cause(source))?;
            let handle = rt.handle().clone();

            Ok((Some(rt), handle))
        }
        Ok(handle) => Ok((None, handle)),
        Err(source) => Err(
            InternalError::new("Unexpected error when getting current Tokio runtime")
                .with_cause(source)
                .into(),
        ),
    }
}

/// Synchronous receiver access must not overlap another connection advancement.
#[cfg(not(feature = "browser"))]
fn get_lock_sync<T>(mutex: &TokioMutex<T>) -> tokio::sync::MutexGuard<'_, T> {
    mutex
        .try_lock()
        .expect("Concurrent connection advancement is unsupported")
}

/// Browser receiver access uses a synchronous mutex.
#[cfg(feature = "browser")]
fn get_lock_sync<T>(mutex: &StdMutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap()
}

/// Async‐lock helper: native = .lock().await, browser = lock().unwrap() inside async fn
#[cfg(not(feature = "browser"))]
async fn get_lock_async<T>(mutex: &TokioMutex<T>) -> tokio::sync::MutexGuard<'_, T> {
    mutex.lock().await
}

/// Async‐lock helper: native = .lock().await, browser = lock().unwrap() inside async fn
#[cfg(feature = "browser")]
pub async fn get_lock_async<T>(mutex: &StdMutex<T>) -> std::sync::MutexGuard<'_, T> {
    // still async, but does the sync lock immediately
    mutex.lock().unwrap()
}

#[derive(Debug)]
pub(crate) enum ParsedMessage<M: SpacetimeModule> {
    TransactionUpdate(M::DbUpdate),
    IdentityToken(Identity, Box<str>, ConnectionId),
    SubscribeApplied {
        query_set_id: QuerySetId,
        initial_update: M::DbUpdate,
    },
    UnsubscribeApplied {
        query_set_id: QuerySetId,
        initial_update: M::DbUpdate,
    },
    SubscriptionError {
        query_set_id: QuerySetId,
        error: String,
    },
    Error(crate::Error),
    TransportError(crate::websocket::WsError),
    SubscribeBatchApplied(ws::v2::SubscribeBatchApplied),
    ReducerResult {
        request_id: u32,
        timestamp: Timestamp,
        result: Result<Result<M::DbUpdate, String>, InternalError>,
    },
    ProcedureResult {
        request_id: u32,
        result: Result<Bytes, InternalError>,
    },
}

#[cfg(not(feature = "browser"))]
fn spawn_parse_loop<M: SpacetimeModule>(
    raw_message_recv: mpsc::UnboundedReceiver<Result<ws::v2::ServerMessage, crate::websocket::WsError>>,
    handle: &runtime::Handle,
    extra_logging: Option<SharedCell<File>>,
) -> (tokio::task::JoinHandle<()>, mpsc::UnboundedReceiver<ParsedMessage<M>>) {
    let (parsed_message_send, parsed_message_recv) = mpsc::unbounded();
    let handle = handle.spawn(parse_loop(raw_message_recv, parsed_message_send, extra_logging));
    (handle, parsed_message_recv)
}

#[cfg(feature = "browser")]
fn spawn_parse_loop<M: SpacetimeModule>(
    raw_message_recv: mpsc::UnboundedReceiver<Result<ws::v2::ServerMessage, crate::websocket::WsError>>,
    extra_logging: Option<SharedCell<File>>,
) -> mpsc::UnboundedReceiver<ParsedMessage<M>> {
    let (parsed_message_send, parsed_message_recv) = mpsc::unbounded();
    wasm_bindgen_futures::spawn_local(parse_loop(raw_message_recv, parsed_message_send, extra_logging));
    parsed_message_recv
}

/// A loop which reads raw WS messages from `recv`, parses them into domain types,
/// and pushes the [`ParsedMessage`]s into `send`.
async fn parse_loop<M: SpacetimeModule>(
    mut recv: mpsc::UnboundedReceiver<Result<ws::v2::ServerMessage, crate::websocket::WsError>>,
    send: mpsc::UnboundedSender<ParsedMessage<M>>,
    extra_logging: Option<SharedCell<File>>,
) {
    while let Some(msg) = recv.next().await {
        debug_log(&extra_logging, |file| {
            writeln!(file, "`parse_loop`: Got raw message: {msg:?}")
        });
        let msg = match msg {
            Ok(msg) => msg,
            Err(error) => {
                let _ = send.unbounded_send(ParsedMessage::TransportError(error));
                break;
            }
        };
        let parsed = match msg {
            ws::v2::ServerMessage::TransactionUpdate(transaction_update) => {
                match M::DbUpdate::parse_update(transaction_update) {
                    Err(e) => ParsedMessage::Error(
                        InternalError::failed_parse("TransactionUpdate", "TransactionUpdate")
                            .with_cause(e)
                            .into(),
                    ),
                    Ok(db_update) => ParsedMessage::TransactionUpdate(db_update),
                }
            }
            ws::v2::ServerMessage::ReducerResult(ws::v2::ReducerResult {
                request_id,
                result,
                timestamp,
            }) => {
                match result {
                    ws::v2::ReducerOutcome::OkEmpty => ParsedMessage::ReducerResult {
                        request_id,
                        timestamp,
                        result: Ok(Ok(M::DbUpdate::default())),
                    },
                    ws::v2::ReducerOutcome::Ok(ws::v2::ReducerOk {
                        ret_value,
                        transaction_update,
                    }) => {
                        if !ret_value.is_empty() {
                            let _ = send.unbounded_send(ParsedMessage::Error(
                                InternalError::new("Non-unit reducer return value").into(),
                            ));
                            break;
                        }
                        match M::DbUpdate::parse_update(transaction_update) {
                            Ok(db_update) => ParsedMessage::ReducerResult {
                                request_id,
                                timestamp,
                                result: Ok(Ok(db_update)),
                            },
                            // Parse errors are not errors with the reducer call itself,
                            // so they don't go to `ParsedMessage::ReducerResult`.
                            // Instead, they go to `ParsedMessage::Error`, as they represent bugs in the SDK.
                            Err(e) => ParsedMessage::Error(
                                InternalError::failed_parse("TransactionUpdate", "ReducerResult")
                                    .with_cause(e)
                                    .into(),
                            ),
                        }
                    }
                    ws::v2::ReducerOutcome::Err(error_return) => match bsatn::from_slice::<String>(&error_return) {
                        Ok(error_message) => ParsedMessage::ReducerResult {
                            request_id,
                            timestamp,
                            result: Ok(Err(error_message)),
                        },
                        // Parse errors are not errors with the reducer call itself,
                        // so they don't go to `ParsedMessage::ReducerResult`.
                        // Instead, they go to `ParsedMessage::Error`, as they represent bugs in the SDK.
                        Err(e) => ParsedMessage::Error(
                            InternalError::failed_parse("String", "ReducerResult")
                                .with_cause(e)
                                .into(),
                        ),
                    },
                    // If the server returns an `InternalError`, that's a module bug, not an SDK bug,
                    // so report it as a `ParsedMessage::ReducerResult`.
                    ws::v2::ReducerOutcome::InternalError(error_message) => ParsedMessage::ReducerResult {
                        request_id,
                        timestamp,
                        result: Err(InternalError::new(error_message)),
                    },
                }
            }
            ws::v2::ServerMessage::InitialConnection(ws::v2::InitialConnection {
                identity,
                token,
                connection_id,
            }) => ParsedMessage::IdentityToken(identity, token, connection_id),
            ws::v2::ServerMessage::OneOffQueryResult(_) => {
                ParsedMessage::Error(InternalError::new("Unexpected one-off query response").into())
            }
            ws::v2::ServerMessage::SubscribeApplied(subscribe_applied) => {
                let db_update = subscribe_applied.rows;
                let query_set_id = subscribe_applied.query_set_id;
                match M::DbUpdate::parse_initial_rows(db_update) {
                    Err(e) => ParsedMessage::Error(
                        InternalError::failed_parse("DbUpdate", "SubscribeApplied")
                            .with_cause(e)
                            .into(),
                    ),
                    Ok(initial_update) => ParsedMessage::SubscribeApplied {
                        query_set_id,
                        initial_update,
                    },
                }
            }
            ws::v2::ServerMessage::UnsubscribeApplied(ws::v2::UnsubscribeApplied {
                query_set_id,
                rows: db_update,
                ..
            }) => {
                let Some(db_update) = db_update else {
                    let _ = send.unbounded_send(ParsedMessage::Error(
                        InternalError::new("Unsubscribe response omitted dropped rows").into(),
                    ));
                    break;
                };
                match M::DbUpdate::parse_unsubscribe_rows(db_update) {
                    Err(e) => ParsedMessage::Error(
                        InternalError::failed_parse("DbUpdate", "UnsubscribeApplied")
                            .with_cause(e)
                            .into(),
                    ),
                    Ok(initial_update) => ParsedMessage::UnsubscribeApplied {
                        query_set_id,
                        initial_update,
                    },
                }
            }
            ws::v2::ServerMessage::SubscriptionError(e) => ParsedMessage::SubscriptionError {
                query_set_id: e.query_set_id,
                error: e.error.to_string(),
            },
            ws::v2::ServerMessage::SubscribeBatchApplied(batch) => ParsedMessage::SubscribeBatchApplied(batch),
            ws::v2::ServerMessage::ProcedureResult(procedure_result) => ParsedMessage::ProcedureResult {
                request_id: procedure_result.request_id,
                result: match procedure_result.status {
                    ws::v2::ProcedureStatus::InternalError(msg) => Err(InternalError::new(msg)),
                    ws::v2::ProcedureStatus::Returned(val) => Ok(val),
                },
            },
        };
        debug_log(&extra_logging, |file| {
            writeln!(file, "`parse_loop`: Parsed as: {parsed:?}")
        });
        if send.unbounded_send(parsed).is_err() {
            break;
        }
    }
}

/// Operations a user can make to a `DbContext` which must be postponed
pub(crate) enum PendingMutation<M: SpacetimeModule> {
    ReconnectReady {
        // Keep the parked receive stream open until this mutation swaps it.
        // Otherwise the async runner can observe EOF before ReconnectReady.
        _park_send: mpsc::UnboundedSender<ParsedMessage<M>>,
        generation: u64,
        token: Option<String>,
        fresh: bool,
        result: Result<
            (
                mpsc::UnboundedSender<ws::v2::ClientMessage>,
                mpsc::UnboundedReceiver<ParsedMessage<M>>,
            ),
            ReconnectFailure,
        >,
    },
    Unsubscribe {
        query_set_id: QuerySetId,
    },
    Subscribe {
        query_set_id: QuerySetId,
        handle: SubscriptionHandleImpl<M>,
    },
    AddInsertCallback {
        table: &'static str,
        callback_id: CallbackId,
        callback: RowCallback<M>,
    },
    RemoveInsertCallback {
        table: &'static str,
        callback_id: CallbackId,
    },
    AddDeleteCallback {
        table: &'static str,
        callback_id: CallbackId,
        callback: RowCallback<M>,
    },
    RemoveDeleteCallback {
        table: &'static str,
        callback_id: CallbackId,
    },
    AddUpdateCallback {
        table: &'static str,
        callback_id: CallbackId,
        callback: UpdateCallback<M>,
    },
    RemoveUpdateCallback {
        table: &'static str,
        callback_id: CallbackId,
    },
    Disconnect,
    InvokeReducerWithCallback {
        reducer: M::Reducer,
        callback: ReducerCallback<M>,
    },
    InvokeProcedureWithCallback {
        procedure: &'static str,
        args: Vec<u8>,
        callback: ProcedureCallback<M>,
    },
}

// Hand-written `Debug` impl, 'cause `SubscriptionHandleImpl` and callbacks aren't printable.
impl<M: SpacetimeModule> std::fmt::Debug for PendingMutation<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PendingMutation::ReconnectReady { generation, .. } => {
                f.debug_tuple("ReconnectReady").field(generation).finish()
            }
            PendingMutation::Unsubscribe { query_set_id } => f
                .debug_struct("PendingMutation::Unsubscribe")
                .field("query_set_id", query_set_id)
                .finish(),
            PendingMutation::Subscribe { query_set_id, .. } => f
                .debug_struct("PendingMutation::Subscribe")
                .field("query_set_id", query_set_id)
                .finish_non_exhaustive(),
            PendingMutation::AddInsertCallback { table, callback_id, .. } => f
                .debug_struct("PendingMutation::AddInsertCallback")
                .field("table", table)
                .field("callback_id", callback_id)
                .finish_non_exhaustive(),
            PendingMutation::RemoveInsertCallback { table, callback_id } => f
                .debug_struct("PendingMutation::RemoveInsertCallback")
                .field("table", table)
                .field("callback_id", callback_id)
                .finish(),
            PendingMutation::AddDeleteCallback { table, callback_id, .. } => f
                .debug_struct("PendingMutation::AddDeleteCallback")
                .field("table", table)
                .field("callback_id", callback_id)
                .finish_non_exhaustive(),
            PendingMutation::RemoveDeleteCallback { table, callback_id } => f
                .debug_struct("PendingMutation::RemoveDeleteCallback")
                .field("table", table)
                .field("callback_id", callback_id)
                .finish(),
            PendingMutation::AddUpdateCallback { table, callback_id, .. } => f
                .debug_struct("PendingMutation::AddUpdateCallback")
                .field("table", table)
                .field("callback_id", callback_id)
                .finish_non_exhaustive(),
            PendingMutation::RemoveUpdateCallback { table, callback_id } => f
                .debug_struct("PendingMutation::RemoveUpdateCallback")
                .field("table", table)
                .field("callback_id", callback_id)
                .finish(),
            PendingMutation::Disconnect => write!(f, "PendingMutation::Disconnect"),
            PendingMutation::InvokeReducerWithCallback { reducer, .. } => f
                .debug_struct("PendingMutation::InvokeReducerWithCallback")
                .field("reducer", reducer)
                .finish_non_exhaustive(),
            PendingMutation::InvokeProcedureWithCallback { procedure, args, .. } => f
                .debug_struct("PendingMutation::InvokeProcedureWithCallback")
                .field("procedure", procedure)
                .field("args", args)
                .finish_non_exhaustive(),
        }
    }
}

enum Message<M: SpacetimeModule> {
    Ws(Option<ParsedMessage<M>>),
    Local(PendingMutation<M>),
}

fn error_is_normal_disconnect(e: &crate::Error) -> bool {
    matches!(e, crate::Error::Disconnected)
}

static NEXT_REQUEST_ID: AtomicU32 = AtomicU32::new(1);

// Get the next request ID to use for a WebSocket message.
pub(crate) fn next_request_id() -> u32 {
    NEXT_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

static NEXT_QUERY_SET_ID: AtomicU32 = AtomicU32::new(1);

// Get the next request ID to use for a WebSocket message.
pub(crate) fn next_query_set_id() -> QuerySetId {
    QuerySetId {
        id: NEXT_QUERY_SET_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    }
}

#[derive(Clone)]
struct ConnectionConfig {
    uri: Uri,
    database_name: String,
    params: WsParams,
}
struct ReconnectState {
    config: ConnectionConfig,
    options: Option<AutomaticReconnectOptions>,
    provider: Option<Arc<dyn TokenProvider>>,
    token: Option<String>,
    attempt: u32,
    generation: u64,
    reconnecting: bool,
    preparing_replay: bool,
    used_fresh_token: bool,
    force_refresh: bool,
    abort: Option<AbortHandle>,
}
impl Drop for ReconnectState {
    fn drop(&mut self) {
        if let Some(abort) = self.abort.take() {
            abort.abort();
        }
    }
}
pub(crate) enum ReconnectFailure {
    Provider(crate::Error),
    Transport(crate::websocket::WsError),
}
