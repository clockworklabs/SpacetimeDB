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
    Event, ReducerEvent, Status,
    __codegen::{InternalError, Reducer},
    callbacks::{
        CallbackId, DbCallbacks, ProcedureCallback, ProcedureCallbacks, ReducerCallback, ReducerCallbacks, RowCallback,
        UpdateCallback,
    },
    client_cache::{ClientCache, TableHandle},
    spacetime_module::{AbstractEventContext, AppliedDiff, DbConnection, DbUpdate, InModule, SpacetimeModule},
    subscription::{PendingUnsubscribeResult, SubscriptionHandleImpl, SubscriptionManager},
    websocket::{WsConnection, WsParams},
};
use bytes::Bytes;
use futures::StreamExt;
use futures_channel::mpsc;
use http::Uri;
use spacetimedb_client_api_messages::websocket::{self as ws, common::QuerySetId};
use spacetimedb_lib::{bsatn, ser::Serialize, ConnectionId, Identity, Timestamp};
use spacetimedb_sats::Deserialize;
use std::sync::{atomic::AtomicU32, Arc, Mutex as StdMutex};
use tokio::{
    runtime::{self, Runtime},
    sync::Mutex as TokioMutex,
};

pub(crate) type SharedCell<T> = Arc<StdMutex<T>>;

/// Implementation of `DbConnection`, `EventContext`,
/// and anything else that provides access to the database connection.
///
/// This must be relatively cheaply `Clone`-able, and have internal sharing,
/// as numerous operations will clone it to get new handles on the connection.
pub struct DbContextImpl<M: SpacetimeModule> {
    runtime: runtime::Handle,

    /// All the state which is safe to hold a lock on while running callbacks.
    pub(crate) inner: SharedCell<DbContextImplInner<M>>,

    /// None if we have disconnected.
    pub(crate) send_chan: SharedCell<Option<mpsc::UnboundedSender<ws::v2::ClientMessage>>>,

    /// The client cache, which stores subscribed rows.
    cache: SharedCell<ClientCache<M>>,

    /// Receiver channel for WebSocket messages,
    /// which are pre-parsed in the background by [`parse_loop`].
    recv: Arc<TokioMutex<mpsc::UnboundedReceiver<ParsedMessage<M>>>>,

    /// Channel into which operations which apparently mutate SDK state,
    /// e.g. registering callbacks, push [`PendingMutation`] messages,
    /// rather than immediately locking the connection and applying their change,
    /// to avoid deadlocks and races.
    pub(crate) pending_mutations_send: mpsc::UnboundedSender<PendingMutation<M>>,

    /// Receive end of `pending_mutations_send`,
    /// from which [Self::apply_pending_mutations] and friends read mutations.
    pending_mutations_recv: Arc<TokioMutex<mpsc::UnboundedReceiver<PendingMutation<M>>>>,

    /// This connection's `Identity`.
    ///
    /// May be `None` if we connected anonymously
    /// and have not yet received the [`ws::v2::InitialConnection`] message.
    identity: SharedCell<Option<Identity>>,

    /// This connection's `ConnectionId`.
    ///
    /// This may be none if we have not yet received the [`ws::v2::InitialConnection`] message.
    connection_id: SharedCell<Option<ConnectionId>>,
}

impl<M: SpacetimeModule> Clone for DbContextImpl<M> {
    fn clone(&self) -> Self {
        Self {
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
        }
    }
}

impl<M: SpacetimeModule> DbContextImpl<M> {
    /// Process a parsed WebSocket message,
    /// applying its mutations to the client cache and invoking callbacks.
    fn process_message(&self, msg: ParsedMessage<M>) -> crate::Result<()> {
        let res = match msg {
            // Error: treat this as an erroneous disconnect.
            ParsedMessage::Error(e) => {
                let disconnect_ctx = self.make_event_ctx(Some(e.clone()));
                self.invoke_disconnected(&disconnect_ctx);
                Err(e)
            }

            // Initial `IdentityToken` message:
            // confirm that the received identity and connection ID are what we expect,
            // store them,
            // then invoke the on_connect callback.
            ParsedMessage::IdentityToken(identity, token, conn_id) => {
                {
                    // Don't hold the `self.identity` lock while running callbacks.
                    // Callbacks can (will) call [`DbContext::identity`], which acquires that lock,
                    // so holding it while running a callback causes deadlocks.
                    let mut ident_store = self.identity.lock().unwrap();
                    if let Some(prev_identity) = *ident_store {
                        assert_eq!(prev_identity, identity);
                    }
                    *ident_store = Some(identity);
                }
                {
                    // Don't hold the `self.connection_id` lock while running callbacks.
                    // Callbacks can (will) call [`DbContext::connection_id`], which acquires that lock,
                    // so holding it while running a callback causes deadlocks.
                    let mut conn_id_store = self.connection_id.lock().unwrap();
                    // This would only happen if the client is using the unstable `set_connection_id` method.
                    if let Some(prev_conn_id) = *conn_id_store {
                        assert_eq!(prev_conn_id, conn_id);
                    }
                    *conn_id_store = Some(conn_id);
                }
                let mut inner = self.inner.lock().unwrap();
                if let Some(on_connect) = inner.on_connect.take() {
                    let ctx = <M::DbConnection as DbConnection>::new(self.clone());
                    on_connect(&ctx, identity, &token);
                }
                Ok(())
            }

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
        };

        res
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

    /// Invoke the on-disconnect callback, and mark [`Self::is_active`] false.
    fn invoke_disconnected(&self, ctx: &M::ErrorContext) {
        let mut inner = self.inner.lock().unwrap();
        // When we disconnect, we first call the on_disconnect method,
        // then we call the `on_error` method for all subscriptions.
        // We don't change the client cache at all.

        // Set `send_chan` to `None`, since `Self::is_active` checks that.
        *self.send_chan.lock().unwrap() = None;

        // Grap the `on_disconnect` callback and invoke it.
        if let Some(disconnect_callback) = inner.on_disconnect.take() {
            disconnect_callback(ctx, ctx.event().clone());
        }

        // Call the `on_disconnect` method for all subscriptions.
        inner.subscriptions.on_disconnect(ctx);
    }

    fn make_event_ctx<E, Ctx: AbstractEventContext<Module = M, Event = E>>(&self, event: E) -> Ctx {
        let imp = self.clone();
        Ctx::new(imp, event)
    }

    /// Apply all queued [`PendingMutation`]s.
    fn apply_pending_mutations(&self) -> crate::Result<()> {
        while let Ok(Some(pending_mutation)) = self.pending_mutations_recv.blocking_lock().try_next() {
            self.apply_mutation(pending_mutation)?;
        }
        Ok(())
    }

    /// Apply an individual [`PendingMutation`].
    fn apply_mutation(&self, mutation: PendingMutation<M>) -> crate::Result<()> {
        match mutation {
            // Subscribe: register the subscription in the [`SubscriptionManager`]
            // and send the `Subscribe` WS message.
            PendingMutation::Subscribe { query_set_id, handle } => {
                let mut inner = self.inner.lock().unwrap();
                // Register the subscription, so we can handle related messages from the server.
                inner.subscriptions.register_subscription(query_set_id, handle.clone());
                if let Some(msg) = handle.start() {
                    self.send_chan
                        .lock()
                        .unwrap()
                        .as_mut()
                        .ok_or(crate::Error::Disconnected)?
                        .unbounded_send(ws::v2::ClientMessage::Subscribe(msg))
                        .expect("Unable to send subscribe message: WS sender loop has dropped its recv channel");
                }
                // else, the handle was already cancelled.
            }

            PendingMutation::Unsubscribe { query_set_id } => {
                let mut inner = self.inner.lock().unwrap();
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
                            .expect("Unable to send unsubscribe message: WS sender loop has dropped its recv channel");
                    }
                }
            }

            // CallReducer: send the `CallReducer` WS message.
            PendingMutation::InvokeReducerWithCallback { reducer, callback } => {
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
                    .expect("Unable to send reducer call message: WS sender loop has dropped its recv channel");
            }

            // Invoke a procedure: stash its callback, then send the `CallProcedure` WS message.
            PendingMutation::InvokeProcedureWithCallback {
                procedure,
                args,
                callback,
            } => {
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
                    .expect("Unable to send procedure call message: WS sender loop has dropped its recv channel");
            }

            // Disconnect: close the connection.
            PendingMutation::Disconnect => {
                // Set `send_chan` to `None`, since `Self::is_active` checks that.
                // This will close the WebSocket loop in websocket.rs,
                // sending a close frame to the server,
                // eventually resulting in disconnect callbacks being called.
                *self.send_chan.lock().unwrap() = None;
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
        let res = match self.recv.blocking_lock().try_next() {
            Ok(None) => {
                let disconnect_ctx = self.make_event_ctx(None);
                self.invoke_disconnected(&disconnect_ctx);
                Err(crate::Error::Disconnected)
            }
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

        let mut pending_mutations = self.pending_mutations_recv.lock().await;
        let mut recv = self.recv.lock().await;

        // Always process pending mutations before WS messages, if they're available,
        // so that newly registered callbacks run on messages.
        // This may be unnecessary, but `tokio::select` does not document any ordering guarantees,
        // and if both `pending_mutations.next()` and `recv.next()` have values ready,
        // we want to process the pending mutation first.
        if let Ok(pending_mutation) = pending_mutations.try_next() {
            return Message::Local(pending_mutation.unwrap());
        }

        tokio::select! {
            pending_mutation = pending_mutations.next() => Message::Local(pending_mutation.unwrap()),
            incoming_message = recv.next() => Message::Ws(incoming_message),
        }
    }

    /// Like [`Self::advance_one_message`], but sleeps the thread until a message is available.
    ///
    /// Called by the autogenerated `DbConnection` method of the same name.
    pub fn advance_one_message_blocking(&self) -> crate::Result<()> {
        match self.runtime.block_on(self.get_message()) {
            Message::Local(pending) => self.apply_mutation(pending),
            Message::Ws(None) => {
                let disconnect_ctx = self.make_event_ctx(None);
                self.invoke_disconnected(&disconnect_ctx);
                Err(crate::Error::Disconnected)
            }
            Message::Ws(Some(msg)) => self.process_message(msg),
        }
    }

    /// Like [`Self::advance_one_message`], but `await`s until a message is available.
    ///
    /// Called by the autogenerated `DbConnection` method of the same name.
    pub async fn advance_one_message_async(&self) -> crate::Result<()> {
        match self.get_message().await {
            Message::Local(pending) => self.apply_mutation(pending),
            Message::Ws(None) => {
                let disconnect_ctx = self.make_event_ctx(None);
                self.invoke_disconnected(&disconnect_ctx);
                Err(crate::Error::Disconnected)
            }
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
        self.send_chan.lock().unwrap().is_some()
    }

    /// Called by the autogenerated `DbConnection` method of the same name.
    pub fn disconnect(&self) -> crate::Result<()> {
        if !self.is_active() {
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

type OnConnectErrorCallback<M> = Box<dyn FnOnce(&<M as SpacetimeModule>::ErrorContext, crate::Error) + Send + 'static>;

type OnDisconnectCallback<M> =
    Box<dyn FnOnce(&<M as SpacetimeModule>::ErrorContext, Option<crate::Error>) + Send + 'static>;

/// All the stuff in a [`DbContextImpl`] which can safely be locked while invoking callbacks.
pub(crate) struct DbContextImplInner<M: SpacetimeModule> {
    /// `Some` if not within the context of an outer runtime. The `Runtime` must
    /// then live as long as `Self`.
    #[allow(unused)]
    runtime: Option<Runtime>,

    db_callbacks: DbCallbacks<M>,
    reducer_callbacks: ReducerCallbacks<M>,
    pub(crate) subscriptions: SubscriptionManager<M>,

    on_connect: Option<OnConnectCallback<M>>,
    #[allow(unused)]
    // TODO: Make use of this to handle `ParsedMessage::Error` before receiving `IdentityToken`.
    on_connect_error: Option<OnConnectErrorCallback<M>>,
    on_disconnect: Option<OnDisconnectCallback<M>>,

    procedure_callbacks: ProcedureCallbacks<M>,
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
    on_connect_error: Option<OnConnectErrorCallback<M>>,
    on_disconnect: Option<OnDisconnectCallback<M>>,

    connection_id_override: Option<ConnectionId>,

    params: WsParams,
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
            on_connect_error: None,
            on_disconnect: None,
            connection_id_override: None,
            params: <_>::default(),
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
- `DbConnection::run_async`.
- `DbConnection::advance_one_message`.
- `DbConnection::advance_one_message_blocking`.
- `DbConnection::advance_one_message_async`.

Which of these methods you should call depends on the specific needs of your application,
but you must call one of them, or else the connection will never progress.
"]
    pub fn build(self) -> crate::Result<M::DbConnection> {
        let imp = self.build_impl()?;
        Ok(<M::DbConnection as DbConnection>::new(imp))
    }

    /// Open a WebSocket connection, build an empty client cache, &c,
    /// to construct a [`DbContextImpl`].
    fn build_impl(self) -> crate::Result<DbContextImpl<M>> {
        let (runtime, handle) = enter_or_create_runtime()?;
        let db_callbacks = DbCallbacks::default();
        let reducer_callbacks = ReducerCallbacks::default();
        let procedure_callbacks = ProcedureCallbacks::default();

        let connection_id_override = self.connection_id_override;
        let ws_connection = tokio::task::block_in_place(|| {
            handle.block_on(WsConnection::connect(
                self.uri.unwrap(),
                self.database_name.as_ref().unwrap(),
                self.token.as_deref(),
                connection_id_override,
                self.params,
            ))
        })
        .map_err(|source| crate::Error::FailedToConnect {
            source: InternalError::new("Failed to initiate WebSocket connection").with_cause(source),
        })?;

        let (_websocket_loop_handle, raw_msg_recv, raw_msg_send) = ws_connection.spawn_message_loop(&handle);
        let (_parse_loop_handle, parsed_recv_chan) = spawn_parse_loop::<M>(raw_msg_recv, &handle);

        let inner = Arc::new(StdMutex::new(DbContextImplInner {
            runtime,

            db_callbacks,
            reducer_callbacks,
            subscriptions: SubscriptionManager::default(),

            on_connect: self.on_connect,
            on_connect_error: self.on_connect_error,
            on_disconnect: self.on_disconnect,
            procedure_callbacks,
        }));

        let mut cache = ClientCache::default();
        M::register_tables(&mut cache);
        let cache = Arc::new(StdMutex::new(cache));
        let send_chan = Arc::new(StdMutex::new(Some(raw_msg_send)));

        let (pending_mutations_send, pending_mutations_recv) = mpsc::unbounded();
        let ctx_imp = DbContextImpl {
            runtime: handle,
            inner,
            send_chan,
            cache,
            recv: Arc::new(TokioMutex::new(parsed_recv_chan)),
            pending_mutations_send,
            pending_mutations_recv: Arc::new(TokioMutex::new(pending_mutations_recv)),
            identity: Arc::new(StdMutex::new(None)),
            connection_id: Arc::new(StdMutex::new(connection_id_override)),
        };

        Ok(ctx_imp)
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
    /// If the passed token is invalid or rejected by the host,
    /// the connection will fail asynchrnonously.
    // FIXME: currently this causes `disconnect` to be called rather than `on_connect_error`.
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

    #[doc(hidden)]
    /// Inject a known connection ID, preventing the server from generating one.
    ///
    /// This functionality is provided for use in SpacetimeDB-cloud.
    /// It is unstable, and will be removed without warning in a future version.
    pub fn with_connection_id(mut self, connection_id: ConnectionId) -> Self {
        self.connection_id_override = Some(connection_id);
        self
    }

    /// Register a callback to run when the connection is successfully initiated.
    ///
    /// The callback will receive three arguments:
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

    /// Register a callback to run when the connection fails asynchronously,
    /// e.g. due to invalid credentials.
    // FIXME: currently never called; `on_disconnect` is called instead.
    pub fn on_connect_error(mut self, callback: impl FnOnce(&M::ErrorContext, crate::Error) + Send + 'static) -> Self {
        if self.on_connect_error.is_some() {
            panic!(
                "DbConnectionBuilder can only register a single `on_connect_error` callback.

Instead of registering multiple `on_connect_error` callbacks, register a single callback which does multiple operations."
            );
        }

        self.on_connect_error = Some(Box::new(callback));
        self
    }

    /// Register a callback to run when the connection is closed.
    // FIXME: currently also called when the connection fails asynchronously, instead of `on_connect_error`.
    pub fn on_disconnect(
        mut self,
        callback: impl FnOnce(&M::ErrorContext, Option<crate::Error>) + Send + 'static,
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

// When called from within an async context, return a handle to it (and no
// `Runtime`), otherwise create a fresh `Runtime` and return it along with a
// handle to it.
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

enum ParsedMessage<M: SpacetimeModule> {
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

fn spawn_parse_loop<M: SpacetimeModule>(
    raw_message_recv: mpsc::UnboundedReceiver<ws::v2::ServerMessage>,
    handle: &runtime::Handle,
) -> (tokio::task::JoinHandle<()>, mpsc::UnboundedReceiver<ParsedMessage<M>>) {
    let (parsed_message_send, parsed_message_recv) = mpsc::unbounded();
    let handle = handle.spawn(parse_loop(raw_message_recv, parsed_message_send));
    (handle, parsed_message_recv)
}

/// A loop which reads raw WS messages from `recv`, parses them into domain types,
/// and pushes the [`ParsedMessage`]s into `send`.
async fn parse_loop<M: SpacetimeModule>(
    mut recv: mpsc::UnboundedReceiver<ws::v2::ServerMessage>,
    send: mpsc::UnboundedSender<ParsedMessage<M>>,
) {
    while let Some(msg) = recv.next().await {
        send.unbounded_send(match msg {
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
                        assert!(
                            ret_value.is_empty(),
                            "Reducer return value should be unit, i.e. 0 bytes, but got {ret_value:?}"
                        );
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
                unreachable!("The Rust SDK does not implement one-off queries")
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
                    unreachable!("The Rust SDK always requests rows to delete when unsubscribing")
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
            ws::v2::ServerMessage::ProcedureResult(procedure_result) => ParsedMessage::ProcedureResult {
                request_id: procedure_result.request_id,
                result: match procedure_result.status {
                    ws::v2::ProcedureStatus::InternalError(msg) => Err(InternalError::new(msg)),
                    ws::v2::ProcedureStatus::Returned(val) => Ok(val),
                },
            },
        })
        .expect("Failed to send ParsedMessage to main thread");
    }
}

/// Operations a user can make to a `DbContext` which must be postponed
pub(crate) enum PendingMutation<M: SpacetimeModule> {
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
