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
    callbacks::{CallbackId, DbCallbacks, ReducerCallback, ReducerCallbacks, RowCallback, UpdateCallback},
    client_cache::{ClientCache, TableHandle},
    spacetime_module::{DbConnection, DbUpdate, EventContext, InModule, SpacetimeModule},
    subscription::{
        OnAppliedCallback, OnErrorCallback, PendingUnsubscribeResult, SubscriptionHandleImpl, SubscriptionManager,
    },
    websocket::{WsConnection, WsParams},
    Event, ReducerEvent, Status,
};
use anyhow::{bail, Context, Result};
use bytes::Bytes;
use futures::StreamExt;
use futures_channel::mpsc;
use http::Uri;
use spacetimedb_client_api_messages::websocket as ws;
use spacetimedb_client_api_messages::websocket::{BsatnFormat, CallReducerFlags, Compression};
use spacetimedb_lib::{bsatn, ser::Serialize, Address, Identity};
use std::{
    collections::HashMap,
    sync::{atomic::AtomicU32, Arc, Mutex as StdMutex, OnceLock},
    time::{Duration, SystemTime},
};
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
    /// and have not yet received the [`ws::IdentityToken`] message.
    identity: SharedCell<Option<Identity>>,
}

impl<M: SpacetimeModule> Clone for DbContextImpl<M> {
    fn clone(&self) -> Self {
        Self {
            runtime: self.runtime.clone(),
            // Being very explicit with `Arc::clone` here,
            // since we'll be doing `DbContextImpl::clone` very frequently,
            // and we need it to be fast.
            inner: Arc::clone(&self.inner),
            cache: Arc::clone(&self.cache),
            recv: Arc::clone(&self.recv),
            pending_mutations_send: self.pending_mutations_send.clone(),
            pending_mutations_recv: Arc::clone(&self.pending_mutations_recv),
            identity: Arc::clone(&self.identity),
        }
    }
}

impl<M: SpacetimeModule> DbContextImpl<M> {
    /// Process a parsed WebSocket message,
    /// applying its mutations to the client cache and invoking callbacks.
    fn process_message(&self, msg: ParsedMessage<M>) -> Result<()> {
        let res = match msg {
            // Error: treat this as an erroneous disconnect.
            ParsedMessage::Error(e) => {
                self.invoke_disconnected(Some(&e));
                Err(e)
            }

            // Initial `IdentityToken` message:
            // confirm that the received identity and address are what we expect,
            // store them,
            // then invoke the on_connect callback.
            ParsedMessage::IdentityToken(identity, token, addr) => {
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
                assert_eq!(get_client_address(), addr);
                let mut inner = self.inner.lock().unwrap();
                if let Some(on_connect) = inner.on_connect.take() {
                    let ctx = <M::DbConnection as DbConnection>::new(self.clone());
                    on_connect(&ctx, identity, &token);
                }
                Ok(())
            }

            // Subscription applied:
            // set the received state to store all the rows,
            // then invoke the on-applied and row callbacks.
            ParsedMessage::InitialSubscription { db_update, sub_id } => {
                // Lock the client cache in a restricted scope,
                // so that it will be unlocked when callbacks run.
                {
                    let mut cache = self.cache.lock().unwrap();
                    // FIXME: delete no-longer-subscribed rows.
                    db_update.apply_to_client_cache(&mut *cache);
                }
                let event_ctx = self.make_event_ctx(Event::SubscribeApplied);
                let mut inner = self.inner.lock().unwrap();
                inner.subscriptions.legacy_subscription_applied(&event_ctx, sub_id);
                // FIXME: invoke delete callbacks for no-longer-subscribed rows.
                db_update.invoke_row_callbacks(&event_ctx, &mut inner.db_callbacks);
                Ok(())
            }

            // Successful transaction update:
            // apply the received diff to the client cache,
            // then invoke on-reducer and row callbacks.
            ParsedMessage::TransactionUpdate(event, Some(update)) => {
                // Lock the client cache in a restricted scope,
                // so that it will be unlocked when callbacks run.
                {
                    let mut cache = self.cache.lock().unwrap();
                    update.apply_to_client_cache(&mut *cache);
                }
                let event_ctx = self.make_event_ctx(event);
                let mut inner = self.inner.lock().unwrap();
                if let Event::Reducer(reducer_event) = event_ctx.event() {
                    inner
                        .reducer_callbacks
                        .invoke_on_reducer(&event_ctx, &reducer_event.reducer);
                }
                update.invoke_row_callbacks(&event_ctx, &mut inner.db_callbacks);
                Ok(())
            }

            // Failed transaction update:
            // invoke on-reducer callbacks.
            ParsedMessage::TransactionUpdate(event, None) => {
                let event_ctx = self.make_event_ctx(event);
                if let Event::Reducer(reducer_event) = event_ctx.event() {
                    let mut inner = self.inner.lock().unwrap();
                    inner
                        .reducer_callbacks
                        .invoke_on_reducer(&event_ctx, &reducer_event.reducer);
                }
                Ok(())
            }
            ParsedMessage::SubscribeApplied {
                query_id,
                initial_update,
            } => {
                // Lock the client cache in a restricted scope,
                // so that it will be unlocked when callbacks run.
                {
                    let mut cache = self.cache.lock().unwrap();
                    initial_update.apply_to_client_cache(&mut *cache);
                }
                let event_ctx = self.make_event_ctx(Event::SubscribeApplied);
                let mut inner = self.inner.lock().unwrap();
                inner.subscriptions.subscription_applied(&event_ctx, query_id);
                // FIXME: invoke delete callbacks for no-longer-subscribed rows.
                initial_update.invoke_row_callbacks(&event_ctx, &mut inner.db_callbacks);
                Ok(())
            }
            ParsedMessage::UnsubscribeApplied {
                query_id,
                initial_update,
            } => {
                // Lock the client cache in a restricted scope,
                // so that it will be unlocked when callbacks run.
                {
                    let mut cache = self.cache.lock().unwrap();
                    initial_update.apply_to_client_cache(&mut *cache);
                }
                let event_ctx = self.make_event_ctx(Event::UnsubscribeApplied);
                let mut inner = self.inner.lock().unwrap();
                inner.subscriptions.unsubscribe_applied(&event_ctx, query_id);
                // FIXME: invoke delete callbacks for no-longer-subscribed rows.
                initial_update.invoke_row_callbacks(&event_ctx, &mut inner.db_callbacks);
                Ok(())
            }
            ParsedMessage::SubscriptionError {
                query_id,
                request_id: _,
                error,
            } => {
                let Some(query_id) = query_id else {
                    // A subscription error that isn't specific to a query is a fatal error.
                    self.invoke_disconnected(Some(&anyhow::anyhow!(error)));
                    return Ok(());
                };
                let mut inner = self.inner.lock().unwrap();
                let event_ctx = self.make_event_ctx(Event::SubscribeError(anyhow::anyhow!(error)));
                inner.subscriptions.subscription_error(&event_ctx, query_id);
                Ok(())
            }
        };

        res
    }

    /// Invoke the on-disconnect callback, and mark [`Self::is_active`] false.
    fn invoke_disconnected(&self, err: Option<&anyhow::Error>) {
        let disconnected_callback = {
            let mut inner = self.inner.lock().unwrap();
            // TODO: Determine correct behavior here.
            // - Delete all rows from client cache?
            // - Invoke `on_disconnect` methods?
            // - End all subscriptions and invoke their `on_error` methods?

            // Set `send_chan` to `None`, since `Self::is_active` checks that.
            inner.send_chan = None;

            // Grap the `on_disconnect` callback and invoke it.
            inner.on_disconnect.take()
        };
        if let Some(disconnect_callback) = disconnected_callback {
            let ctx = <M::DbConnection as DbConnection>::new(self.clone());
            disconnect_callback(&ctx, err);
        }
    }

    fn make_event_ctx(&self, event: Event<M::Reducer>) -> M::EventContext {
        let imp = self.clone();
        <M::EventContext as EventContext>::new(imp, event)
    }

    /// Apply all queued [`PendingMutation`]s.
    fn apply_pending_mutations(&self) -> anyhow::Result<()> {
        while let Ok(Some(pending_mutation)) = self.pending_mutations_recv.blocking_lock().try_next() {
            self.apply_mutation(pending_mutation)?;
        }
        Ok(())
    }

    /// Apply an individual [`PendingMutation`].
    fn apply_mutation(&self, mutation: PendingMutation<M>) -> anyhow::Result<()> {
        match mutation {
            // Subscribe: register the subscription in the [`SubscriptionManager`]
            // and send the `Subscribe` WS message.
            PendingMutation::Subscribe {
                on_applied,
                queries,
                sub_id,
                on_error,
            } => {
                let mut inner = self.inner.lock().unwrap();
                inner
                    .subscriptions
                    .register_legacy_subscription(sub_id, on_applied, on_error);
                inner
                    .send_chan
                    .as_mut()
                    .ok_or(DisconnectedError {})?
                    .unbounded_send(ws::ClientMessage::Subscribe(ws::Subscribe {
                        query_strings: queries,
                        request_id: sub_id,
                    }))
                    .expect("Unable to send subscribe message: WS sender loop has dropped its recv channel");
            }
            // Subscribe: register the subscription in the [`SubscriptionManager`]
            // and send the `Subscribe` WS message.
            PendingMutation::SubscribeSingle { query_id, handle } => {
                let mut inner = self.inner.lock().unwrap();
                inner.subscriptions.register_subscription(query_id, handle.clone());
                if let Some(msg) = handle.start() {
                    inner
                        .send_chan
                        .as_mut()
                        .ok_or(DisconnectedError {})?
                        .unbounded_send(ws::ClientMessage::SubscribeSingle(msg))
                        .expect("Unable to send subscribe message: WS sender loop has dropped its recv channel");
                }
                // else, the handle was already cancelled.
            }

            PendingMutation::Unsubscribe { query_id } => {
                let mut inner = self.inner.lock().unwrap();
                match inner.subscriptions.handle_pending_unsubscribe(query_id) {
                    PendingUnsubscribeResult::DoNothing =>
                    // The subscription was already unsubscribed, so we don't need to send an unsubscribe message.
                    {
                        return Ok(())
                    }

                    PendingUnsubscribeResult::RunCallback(callback) => {
                        callback(&self.make_event_ctx(Event::UnsubscribeApplied));
                    }
                    PendingUnsubscribeResult::SendUnsubscribe(m) => {
                        inner
                            .send_chan
                            .as_mut()
                            .ok_or(DisconnectedError {})?
                            .unbounded_send(ws::ClientMessage::Unsubscribe(m))
                            .expect("Unable to send unsubscribe message: WS sender loop has dropped its recv channel");
                    }
                }
            }

            // CallReducer: send the `CallReducer` WS message.
            PendingMutation::CallReducer { reducer, args_bsatn } => {
                let inner = &mut *self.inner.lock().unwrap();

                let flags = inner.call_reducer_flags.get_flags(reducer);
                let msg = ws::ClientMessage::CallReducer(ws::CallReducer {
                    reducer: reducer.into(),
                    args: args_bsatn.into(),
                    request_id: 0,
                    flags,
                });
                inner
                    .send_chan
                    .as_mut()
                    .ok_or(DisconnectedError {})?
                    .unbounded_send(msg)
                    .expect("Unable to send reducer call message: WS sender loop has dropped its recv channel");
            }

            // Disconnect: close the connection.
            PendingMutation::Disconnect => {
                // Set `send_chan` to `None`, since `Self::is_active` checks that.
                // This will close the WebSocket loop in websocket.rs,
                // sending a close frame to the server,
                // eventually resulting in disconnect callbacks being called.
                self.inner.lock().unwrap().send_chan = None;
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
            PendingMutation::AddReducerCallback {
                reducer,
                callback_id,
                callback,
            } => {
                self.inner
                    .lock()
                    .unwrap()
                    .reducer_callbacks
                    .register_on_reducer(reducer, callback_id, callback);
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
            PendingMutation::RemoveReducerCallback { reducer, callback_id } => {
                self.inner
                    .lock()
                    .unwrap()
                    .reducer_callbacks
                    .remove_on_reducer(reducer, callback_id);
            }
            PendingMutation::SetCallReducerFlags {
                reducer: reducer_name,
                flags,
            } => {
                self.inner
                    .lock()
                    .unwrap()
                    .call_reducer_flags
                    .set_flags(reducer_name, flags);
            }
        };
        Ok(())
    }

    /// If a WebSocket message is waiting, process it and return `true`.
    /// If no WebSocket messages are in the queue, immediately return `false`.
    ///
    /// Called by the autogenerated `DbConnection` method of the same name.
    pub fn advance_one_message(&self) -> Result<bool> {
        // Apply any pending mutations before processing a WS message,
        // so that pending callbacks don't get skipped.
        self.apply_pending_mutations()?;

        // Deranged behavior: mpsc's `try_next` returns `Ok(None)` when the channel is closed,
        // and `Err(_)` when the channel is open and waiting. This seems exactly backwards.
        let res = match self.recv.blocking_lock().try_next() {
            Ok(None) => {
                self.invoke_disconnected(None);
                Err(anyhow::Error::new(DisconnectedError {}))
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
    pub fn advance_one_message_blocking(&self) -> Result<()> {
        match self.runtime.block_on(self.get_message()) {
            Message::Local(pending) => self.apply_mutation(pending),
            Message::Ws(None) => {
                self.invoke_disconnected(None);
                Err(anyhow::Error::new(DisconnectedError {}))
            }
            Message::Ws(Some(msg)) => self.process_message(msg),
        }
    }

    /// Like [`Self::advance_one_message`], but `await`s until a message is available.
    ///
    /// Called by the autogenerated `DbConnection` method of the same name.
    pub async fn advance_one_message_async(&self) -> Result<()> {
        match self.get_message().await {
            Message::Local(pending) => self.apply_mutation(pending),
            Message::Ws(None) => {
                self.invoke_disconnected(None);
                Err(anyhow::Error::new(DisconnectedError {}))
            }
            Message::Ws(Some(msg)) => self.process_message(msg),
        }
    }

    /// Call [`Self::advance_one_message`] in a loop until no more messages are waiting.
    ///
    /// Called by the autogenerated `DbConnection` method of the same name.
    pub fn frame_tick(&self) -> Result<()> {
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
    pub async fn run_async(&self) -> Result<()> {
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
        self.inner.lock().unwrap().send_chan.is_some()
    }

    /// Called by the autogenerated `DbConnection` method of the same name.
    pub fn disconnect(&self) -> Result<()> {
        if !self.is_active() {
            bail!("Already disconnected in call to `DbContext::disconnect`");
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
    pub fn call_reducer<Args: Serialize + InModule<Module = M>>(
        &self,
        reducer_name: &'static str,
        args: Args,
    ) -> Result<()> {
        // TODO(centril, perf): consider using a thread local pool to avoid allocating each time.
        let args_bsatn = bsatn::to_vec(&args)
            .with_context(|| format!("Failed to BSATN serialize arguments for reducer {reducer_name}"))?;

        self.queue_mutation(PendingMutation::CallReducer {
            reducer: reducer_name,
            args_bsatn,
        });
        Ok(())
    }

    /// Called by autogenerated on `reducer_config` methods.
    pub fn set_call_reducer_flags(&self, reducer: &'static str, flags: CallReducerFlags) {
        self.queue_mutation(PendingMutation::SetCallReducerFlags { reducer, flags });
    }

    /// Called by autogenerated reducer callback methods.
    pub fn on_reducer(&self, reducer_name: &'static str, callback: ReducerCallback<M>) -> CallbackId {
        let callback_id = CallbackId::get_next();
        self.queue_mutation(PendingMutation::AddReducerCallback {
            reducer: reducer_name,
            callback_id,
            callback,
        });
        callback_id
    }

    /// Called by autogenerated reducer callback methods.
    pub fn remove_on_reducer(&self, reducer_name: &'static str, callback: CallbackId) {
        self.queue_mutation(PendingMutation::RemoveReducerCallback {
            reducer: reducer_name,
            callback_id: callback,
        });
    }

    /// Called by the autogenerated `DbConnection` method of the same name.
    pub fn try_identity(&self) -> Option<Identity> {
        *self.identity.lock().unwrap()
    }

    /// Called by the autogenerated `DbConnection` method of the same name.
    pub fn address(&self) -> Address {
        get_client_address()
    }
}

type OnConnectCallback<M> = Box<dyn FnOnce(&<M as SpacetimeModule>::DbConnection, Identity, &str) + Send + 'static>;

type OnConnectErrorCallback = Box<dyn FnOnce(&anyhow::Error) + Send + 'static>;

type OnDisconnectCallback<M> =
    Box<dyn FnOnce(&<M as SpacetimeModule>::DbConnection, Option<&anyhow::Error>) + Send + 'static>;

/// All the stuff in a [`DbContextImpl`] which can safely be locked while invoking callbacks.
pub(crate) struct DbContextImplInner<M: SpacetimeModule> {
    /// `Some` if not within the context of an outer runtime. The `Runtime` must
    /// then live as long as `Self`.
    #[allow(unused)]
    runtime: Option<Runtime>,

    /// None if we have disconnected.
    send_chan: Option<mpsc::UnboundedSender<ws::ClientMessage<Bytes>>>,

    db_callbacks: DbCallbacks<M>,
    reducer_callbacks: ReducerCallbacks<M>,
    pub(crate) subscriptions: SubscriptionManager<M>,

    on_connect: Option<OnConnectCallback<M>>,
    #[allow(unused)]
    // TODO: Make use of this to handle `ParsedMessage::Error` before receiving `IdentityToken`.
    on_connect_error: Option<OnConnectErrorCallback>,
    on_disconnect: Option<OnDisconnectCallback<M>>,

    call_reducer_flags: CallReducerFlagsMap,
}

/// Maps reducer names to the flags to use for `.call_reducer(..)`.
#[derive(Default, Clone)]
struct CallReducerFlagsMap {
    // TODO(centril): consider replacing the string with a type-id based map
    // where each reducer is associated with a marker type.
    map: HashMap<&'static str, CallReducerFlags>,
}

impl CallReducerFlagsMap {
    /// Returns the [`CallReducerFlags`] for `reducer_name`.
    fn get_flags(&self, reducer_name: &str) -> CallReducerFlags {
        self.map.get(reducer_name).copied().unwrap_or_default()
    }

    /// Sets the [`CallReducerFlags`] for `reducer_name` to `flags`.
    pub fn set_flags(&mut self, reducer_name: &'static str, flags: CallReducerFlags) {
        if flags == <_>::default() {
            self.map.remove(reducer_name)
        } else {
            self.map.insert(reducer_name, flags)
        };
    }
}

/// A builder-pattern constructor for a `DbConnection` connection to the module `M`.
///
/// `M` will be the autogenerated opaque module type.
///
/// Get a builder by calling `DbConnection::builder()`.
// TODO: Move into its own module which is not #[doc(hidden)]?
pub struct DbConnectionBuilder<M: SpacetimeModule> {
    uri: Option<Uri>,

    module_name: Option<String>,

    credentials: Option<(Identity, String)>,

    on_connect: Option<OnConnectCallback<M>>,
    on_connect_error: Option<OnConnectErrorCallback>,
    on_disconnect: Option<OnDisconnectCallback<M>>,

    params: WsParams,
}

/// This process's global client address, which will be attacked to all connections it makes.
static CLIENT_ADDRESS: OnceLock<Address> = OnceLock::new();

fn get_client_address() -> Address {
    *CLIENT_ADDRESS.get_or_init(|| Address::from_byte_array(rand::random()))
}

#[doc(hidden)]
/// Attempt to set this process's client address to a known value.
///
/// This functionality is exposed for use in SpacetimeDB-cloud.
/// It is unstable, and will be removed without warning in a future version.
///
/// Clients which want a particular client address must call this method
/// before constructing any connection.
/// Once any connection is constructed, the per-process client address value is locked in,
/// and cannot be overwritten.
///
/// Returns `Err` if this process's client address has already been initialized to a random value.
pub fn set_client_address(addr: Address) -> Result<()> {
    let stored = *CLIENT_ADDRESS.get_or_init(|| addr);
    anyhow::ensure!(
        stored == addr,
        "Call to set_client_address after CLIENT_ADDRESS was initialized to a different value"
    );
    Ok(())
}

impl<M: SpacetimeModule> DbConnectionBuilder<M> {
    /// Implementation of the generated `DbConnection::builder` method.
    /// Call that method instead.
    #[doc(hidden)]
    pub fn new() -> Self {
        Self {
            uri: None,
            module_name: None,
            credentials: None,
            on_connect: None,
            on_connect_error: None,
            on_disconnect: None,
            params: <_>::default(),
        }
    }

    /// Open a WebSocket connection to the remote module,
    /// with all configuration and callbacks registered in the builder `self`.
    ///
    /// This method panics if `self` lacks a required configuration,
    /// or returns an `Err` if some I/O operation during the initial WebSocket connection fails.
    ///
    /// Successful return from this method does not necessarily imply a valid `DbConnection`;
    /// the connection may still fail asynchronously,
    /// leading to the [`Self::on_connect_error`] callback being invoked.
    ///
    /// Before calling this method, make sure to invoke at least [`Self::with_uri`] and [`Self::with_module_name`]
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
    pub fn build(self) -> Result<M::DbConnection> {
        let imp = self.build_impl()?;
        Ok(<M::DbConnection as DbConnection>::new(imp))
    }

    /// Open a WebSocket connection, build an empty client cache, &c,
    /// to construct a [`DbContextImpl`].
    fn build_impl(self) -> Result<DbContextImpl<M>> {
        let (runtime, handle) = enter_or_create_runtime()?;
        let db_callbacks = DbCallbacks::default();
        let reducer_callbacks = ReducerCallbacks::default();

        let ws_connection = tokio::task::block_in_place(|| {
            handle.block_on(WsConnection::connect(
                self.uri.unwrap(),
                self.module_name.as_ref().unwrap(),
                self.credentials.as_ref(),
                get_client_address(),
                self.params,
            ))
        })?;

        let (_websocket_loop_handle, raw_msg_recv, raw_msg_send) = ws_connection.spawn_message_loop(&handle);
        let (_parse_loop_handle, parsed_recv_chan) = spawn_parse_loop::<M>(raw_msg_recv, &handle);

        let inner = Arc::new(StdMutex::new(DbContextImplInner {
            runtime,

            send_chan: Some(raw_msg_send),
            db_callbacks,
            reducer_callbacks,
            subscriptions: SubscriptionManager::default(),

            on_connect: self.on_connect,
            on_connect_error: self.on_connect_error,
            on_disconnect: self.on_disconnect,
            call_reducer_flags: <_>::default(),
        }));

        let mut cache = ClientCache::default();
        M::register_tables(&mut cache);
        let cache = Arc::new(StdMutex::new(cache));

        let (pending_mutations_send, pending_mutations_recv) = mpsc::unbounded();
        let ctx_imp = DbContextImpl {
            runtime: handle,
            inner,
            cache,
            recv: Arc::new(TokioMutex::new(parsed_recv_chan)),
            pending_mutations_send,
            pending_mutations_recv: Arc::new(TokioMutex::new(pending_mutations_recv)),
            identity: Arc::new(StdMutex::new(self.credentials.as_ref().map(|creds| creds.0))),
        };

        Ok(ctx_imp)
    }

    /// Set the URI of the SpacetimeDB host which is running the remote module.
    ///
    /// The URI must have either no scheme or one of the schemes `http`, `https`, `ws` or `wss`.
    pub fn with_uri<E: std::fmt::Debug>(mut self, uri: impl TryInto<Uri, Error = E>) -> Self {
        let uri = uri.try_into().expect("Unable to parse supplied URI");
        self.uri = Some(uri);
        self
    }

    /// Set the name or identity of the remote module.
    pub fn with_module_name(mut self, name_or_identity: impl ToString) -> Self {
        self.module_name = Some(name_or_identity.to_string());
        self
    }

    /// Set the credentials with which to connect to the remote database.
    ///
    /// If `credentials` is `None` or this method is not invoked,
    /// the SpacetimeDB host will generate a new anonymous `Identity`.
    ///
    /// If the passed token is invalid, is not recognized by the host,
    /// or does not authenticate as the passed `Identity`,
    /// the connection will fail asynchrnonously.
    // FIXME: currently this causes `disconnect` to be called rather than `on_connect_error`.
    pub fn with_credentials(mut self, credentials: Option<(Identity, String)>) -> Self {
        self.credentials = credentials;
        self
    }

    /// Sets the compression used when a certain threshold in the message size has been reached.
    ///
    /// The current threshold used by the host is 1KiB for the entire server message
    /// and for individual query updates.
    /// Note however that this threshold is not guaranteed and may change without notice.
    pub fn with_compression(mut self, compression: Compression) -> Self {
        self.params.compression = compression;
        self
    }

    /// Sets whether the "light" mode is used.
    ///
    /// The light mode is meant for clients which are network-bandwidth constrained
    /// and results in the following:
    /// - Incremental updates will not include information about the reducer that caused them.
    /// - The client will not be notified about a reducer the client called
    ///   without being subscribed to any relevant queries.
    pub fn with_light_mode(mut self, light: bool) -> Self {
        self.params.light = light;
        self
    }

    /// Register a callback to run when the connection is successfully initiated.
    ///
    /// The callback will receive three arguments:
    /// - The `DbConnection` which has successfully connected.
    /// - The `Identity` of the successful connection.
    ///   If an identity and token were passed to [`Self::with_credentials`], this will be the same `Identity`.
    /// - The private access token which can be used to later re-authenticate as the same `Identity`.
    ///   If an identity and token were passed to [`Self::with_credentials`],
    ///   this may not be string-equal to the supplied token, but will authenticate as the same `Identity`.
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
    pub fn on_connect_error(mut self, callback: impl FnOnce(&anyhow::Error) + Send + 'static) -> Self {
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
        callback: impl FnOnce(&M::DbConnection, Option<&anyhow::Error>) + Send + 'static,
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
fn enter_or_create_runtime() -> Result<(Option<Runtime>, runtime::Handle)> {
    match runtime::Handle::try_current() {
        Err(e) if e.is_missing_context() => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(1)
                .thread_name("spacetimedb-background-connection")
                .build()?;
            let handle = rt.handle().clone();

            Ok((Some(rt), handle))
        }
        Ok(handle) => Ok((None, handle)),
        Err(e) => Err(e.into()),
    }
}

enum ParsedMessage<M: SpacetimeModule> {
    InitialSubscription {
        db_update: M::DbUpdate,
        sub_id: u32,
    },
    TransactionUpdate(Event<M::Reducer>, Option<M::DbUpdate>),
    IdentityToken(Identity, Box<str>, Address),
    SubscribeApplied {
        query_id: u32,
        initial_update: M::DbUpdate,
    },
    UnsubscribeApplied {
        query_id: u32,
        initial_update: M::DbUpdate,
    },
    SubscriptionError {
        query_id: Option<u32>,
        request_id: Option<u32>,
        error: String,
    },
    Error(anyhow::Error),
}

fn spawn_parse_loop<M: SpacetimeModule>(
    raw_message_recv: mpsc::UnboundedReceiver<ws::ServerMessage<BsatnFormat>>,
    handle: &runtime::Handle,
) -> (tokio::task::JoinHandle<()>, mpsc::UnboundedReceiver<ParsedMessage<M>>) {
    let (parsed_message_send, parsed_message_recv) = mpsc::unbounded();
    let handle = handle.spawn(parse_loop(raw_message_recv, parsed_message_send));
    (handle, parsed_message_recv)
}

/// A loop which reads raw WS messages from `recv`, parses them into domain types,
/// and pushes the [`ParsedMessage`]s into `send`.
async fn parse_loop<M: SpacetimeModule>(
    mut recv: mpsc::UnboundedReceiver<ws::ServerMessage<BsatnFormat>>,
    send: mpsc::UnboundedSender<ParsedMessage<M>>,
) {
    while let Some(msg) = recv.next().await {
        send.unbounded_send(match msg {
            ws::ServerMessage::InitialSubscription(sub) => M::DbUpdate::try_from(sub.database_update)
                .map(|update| ParsedMessage::InitialSubscription {
                    db_update: update,
                    sub_id: sub.request_id,
                })
                .unwrap_or_else(|e| {
                    ParsedMessage::Error(e.context("Failed to parse DbUpdate from InitialSubscription"))
                }),
            ws::ServerMessage::TransactionUpdate(ws::TransactionUpdate {
                status,
                timestamp,
                caller_identity,
                caller_address,
                reducer_call,
                energy_quanta_used,
                ..
            }) => match Status::parse_status_and_update::<M>(status) {
                Err(e) => ParsedMessage::Error(e.context("Failed to parse Status from TransactionUpdate")),
                Ok((status, db_update)) => {
                    let event = M::Reducer::try_from(reducer_call)
                        .map(|reducer| {
                            Event::Reducer(ReducerEvent {
                                caller_address: caller_address.none_if_zero(),
                                caller_identity,
                                energy_consumed: Some(energy_quanta_used.quanta),
                                timestamp: SystemTime::UNIX_EPOCH
                                    .checked_add(Duration::from_micros(timestamp.microseconds))
                                    .unwrap(),
                                reducer,
                                status,
                            })
                        })
                        .unwrap_or(Event::UnknownTransaction);
                    ParsedMessage::TransactionUpdate(event, db_update)
                }
            },
            ws::ServerMessage::TransactionUpdateLight(ws::TransactionUpdateLight { update, request_id: _ }) => {
                match M::DbUpdate::parse_update(update) {
                    Err(e) => ParsedMessage::Error(e.context("Failed to parse update from TransactionUpdateLight")),
                    Ok(db_update) => ParsedMessage::TransactionUpdate(Event::UnknownTransaction, Some(db_update)),
                }
            }
            ws::ServerMessage::IdentityToken(ws::IdentityToken {
                identity,
                token,
                address,
            }) => ParsedMessage::IdentityToken(identity, token, address),
            ws::ServerMessage::OneOffQueryResponse(_) => {
                unreachable!("The Rust SDK does not implement one-off queries")
            }
            ws::ServerMessage::SubscribeApplied(subscribe_applied) => {
                let table_rows = subscribe_applied.rows.table_rows;
                let db_update = ws::DatabaseUpdate::from_iter(std::iter::once(table_rows));
                let query_id = subscribe_applied.query_id.id;
                match M::DbUpdate::parse_update(db_update) {
                    Err(e) => ParsedMessage::Error(e.context("Failed to parse update from SubscribeApplied")),
                    Ok(initial_update) => ParsedMessage::SubscribeApplied {
                        query_id,
                        initial_update,
                    },
                }
            }
            ws::ServerMessage::UnsubscribeApplied(unsubscribe_applied) => {
                let table_rows = unsubscribe_applied.rows.table_rows;
                let db_update = ws::DatabaseUpdate::from_iter(std::iter::once(table_rows));
                let query_id = unsubscribe_applied.query_id.id;
                match M::DbUpdate::parse_update(db_update) {
                    Err(e) => ParsedMessage::Error(e.context("Failed to parse update from SubscribeApplied")),
                    Ok(initial_update) => ParsedMessage::UnsubscribeApplied {
                        query_id,
                        initial_update,
                    },
                }
            }
            ws::ServerMessage::SubscriptionError(e) => {
                ParsedMessage::SubscriptionError {
                    query_id: e.query_id,
                    request_id: e.request_id,
                    error: e.error.to_string(),
                }
            }
        })
        .expect("Failed to send ParsedMessage to main thread");
    }
}

/// Operations a user can make to a `DbContext` which must be postponed
pub(crate) enum PendingMutation<M: SpacetimeModule> {
    Subscribe {
        on_applied: Option<OnAppliedCallback<M>>,
        on_error: Option<OnErrorCallback<M>>,
        queries: Box<[Box<str>]>,
        // TODO: replace `queries` with query_sql: String,
        sub_id: u32,
    },
    Unsubscribe {
        query_id: u32,
    },
    SubscribeSingle {
        // query: Box<str>,
        query_id: u32,
        handle: SubscriptionHandleImpl<M>,
    },
    // TODO: Unsubscribe { ??? },
    CallReducer {
        reducer: &'static str,
        args_bsatn: Vec<u8>,
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
    AddReducerCallback {
        reducer: &'static str,
        callback_id: CallbackId,
        callback: ReducerCallback<M>,
    },
    RemoveReducerCallback {
        reducer: &'static str,
        callback_id: CallbackId,
    },
    Disconnect,
    SetCallReducerFlags {
        reducer: &'static str,
        flags: CallReducerFlags,
    },
}

enum Message<M: SpacetimeModule> {
    Ws(Option<ParsedMessage<M>>),
    Local(PendingMutation<M>),
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DisconnectedError {}

impl std::fmt::Display for DisconnectedError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Disconnected")
    }
}

impl std::error::Error for DisconnectedError {}

fn error_is_normal_disconnect(e: &anyhow::Error) -> bool {
    e.is::<DisconnectedError>()
}

static NEXT_REQUEST_ID: AtomicU32 = AtomicU32::new(1);

// Get the next request ID to use for a WebSocket message.
pub(crate) fn next_request_id() -> u32 {
    NEXT_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

static NEXT_SUBSCRIPTION_ID: AtomicU32 = AtomicU32::new(1);

// Get the next request ID to use for a WebSocket message.
pub(crate) fn next_subscription_id() -> u32 {
    NEXT_SUBSCRIPTION_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    #[test]
    fn dummy() {
        assert_eq!(1, 1);
    }
}
