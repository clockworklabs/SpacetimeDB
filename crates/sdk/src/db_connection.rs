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
    client_cache::{ClientCache, ClientCacheView, TableCache, UniqueConstraint},
    spacetime_module::{DbConnection, DbUpdate, EventContext, InModule, SpacetimeModule},
    subscription::{OnAppliedCallback, OnErrorCallback, SubscriptionManager},
    websocket::WsConnection,
    ws_messages as ws, Event, ReducerEvent, Status,
};
use anyhow::{bail, Context, Result};
use futures::StreamExt;
use futures_channel::mpsc;
use http::Uri;
use spacetimedb_lib::{bsatn, de::Deserialize, ser::Serialize, Address, Identity};
use std::{
    sync::{Arc, Mutex as StdMutex, OnceLock},
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

    /// The most recent client cache state.
    cache: SharedCell<ClientCacheView<M>>,

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
                let prev_cache_view = Arc::clone(&*self.cache.lock().unwrap());
                let mut new_cache = ClientCache::clone(&*prev_cache_view);
                // FIXME: delete no-longer-subscribed rows.
                db_update.apply_to_client_cache(&mut new_cache);
                let new_cache_view = Arc::new(new_cache);
                *self.cache.lock().unwrap() = new_cache_view;
                let event_ctx = self.make_event_ctx(Event::SubscribeApplied);
                let mut inner = self.inner.lock().unwrap();
                inner.subscriptions.subscription_applied(&event_ctx, sub_id);
                // FIXME: invoke delete callbacks for no-longer-subscribed rows.
                db_update.invoke_row_callbacks(&event_ctx, &mut inner.db_callbacks);
                Ok(())
            }

            // Successful transaction update:
            // apply the received diff to the client cache,
            // then invoke on-reducer and row callbacks.
            ParsedMessage::TransactionUpdate(event, Some(update)) => {
                let prev_cache_view = Arc::clone(&*self.cache.lock().unwrap());
                let mut new_cache = ClientCache::clone(&*prev_cache_view);
                update.apply_to_client_cache(&mut new_cache);
                let new_cache_view = Arc::new(new_cache);
                *self.cache.lock().unwrap() = new_cache_view;
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
            let ctx = M::DbConnection::new(self.clone());
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
                inner.subscriptions.register_subscription(sub_id, on_applied, on_error);
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

            // CallReducer: send the `CallReducer` WS message.
            PendingMutation::CallReducer { reducer, args_bsatn } => {
                let msg = ws::ClientMessage::CallReducer(ws::CallReducer {
                    reducer: reducer.to_string(),
                    args: ws::EncodedValue::Binary(args_bsatn.into()),
                    request_id: 0,
                });
                self.inner
                    .lock()
                    .unwrap()
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
        let table_view = self
            .cache
            .lock()
            .unwrap()
            .get_table::<Row>(table_name)
            .cloned()
            .unwrap_or_else(|| Arc::new(TableCache::default()));
        let pending_mutations = self.pending_mutations_send.clone();
        TableHandle {
            table_view,
            pending_mutations,
            table: table_name,
        }
    }

    /// Called by autogenerated reducer invokation methods.
    pub fn call_reducer<Args: Serialize + InModule<Module = M>>(
        &self,
        reducer_name: &'static str,
        args: Args,
    ) -> Result<()> {
        let args_bsatn = bsatn::to_vec(&args)
            .with_context(|| format!("Failed to BSATN serialize arguments for reducer {reducer_name}"))?;
        self.queue_mutation(PendingMutation::CallReducer {
            reducer: reducer_name,
            args_bsatn,
        });
        Ok(())
    }

    /// Called by autogenerated reducer callback methods.
    pub fn on_reducer<Args: Deserialize<'static> + InModule<Module = M> + 'static>(
        &self,
        reducer_name: &'static str,
        mut callback: impl FnMut(&M::EventContext, &Args) + Send + 'static,
    ) -> CallbackId {
        let callback_id = CallbackId::get_next();
        self.queue_mutation(PendingMutation::AddReducerCallback {
            reducer: reducer_name,
            callback_id,
            callback: Box::new(move |ctx, args| {
                let args = args.downcast_ref::<Args>().unwrap();
                callback(ctx, args);
            }),
        });
        callback_id
    }

    /// Called by autogenerated reducer callback methods.
    pub fn remove_on_reducer<Args: InModule<Module = M>>(&self, reducer_name: &'static str, callback: CallbackId) {
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
    send_chan: Option<mpsc::UnboundedSender<ws::ClientMessage>>,

    db_callbacks: DbCallbacks<M>,
    reducer_callbacks: ReducerCallbacks<M>,
    pub(crate) subscriptions: SubscriptionManager<M>,

    on_connect: Option<OnConnectCallback<M>>,
    #[allow(unused)]
    // TODO: Make use of this to handle `ParsedMessage::Error` before receiving `IdentityToken`.
    on_connect_error: Option<OnConnectErrorCallback>,
    on_disconnect: Option<OnDisconnectCallback<M>>,
}

/// Internal implementation of a generated `TableHandle` struct,
/// which mediates access to a table in the client cache.
pub struct TableHandle<Row: InModule> {
    /// May be `None` if there are no rows in the table cache.
    table_view: Arc<TableCache<Row>>,
    /// Handle on the connection's `pending_mutations_send` channel,
    /// so we can send callback-related [`PendingMutation`] messages.
    pending_mutations: mpsc::UnboundedSender<PendingMutation<Row::Module>>,

    /// The name of the table.
    table: &'static str,
}

impl<Row: InModule + Send + Sync + Clone + 'static> TableHandle<Row> {
    /// Called by the autogenerated implementation of the [`crate::Table`] method of the same name.
    pub fn count(&self) -> u64 {
        self.table_view.entries.len() as u64
    }

    /// Called by the autogenerated implementation of the [`crate::Table`] method of the same name.
    pub fn iter(&self) -> impl Iterator<Item = Row> + '_ {
        self.table_view.entries.values().cloned()
    }

    /// See [`DbContextImpl::queue_mutation`].
    fn queue_mutation(&self, mutation: PendingMutation<Row::Module>) {
        self.pending_mutations.unbounded_send(mutation).unwrap();
    }

    /// Called by the autogenerated implementation of the [`crate::Table`] method of the same name.
    pub fn on_insert(
        &self,
        mut callback: impl FnMut(&<Row::Module as SpacetimeModule>::EventContext, &Row) + Send + 'static,
    ) -> CallbackId {
        let callback_id = CallbackId::get_next();
        self.queue_mutation(PendingMutation::AddInsertCallback {
            table: self.table,
            callback: Box::new(move |ctx, row| {
                let row = row.downcast_ref::<Row>().unwrap();
                callback(ctx, row);
            }),
            callback_id,
        });
        callback_id
    }

    /// Called by the autogenerated implementation of the [`crate::Table`] method of the same name.
    pub fn remove_on_insert(&self, callback: CallbackId) {
        self.queue_mutation(PendingMutation::RemoveInsertCallback {
            table: self.table,
            callback_id: callback,
        });
    }

    /// Called by the autogenerated implementation of the [`crate::Table`] method of the same name.
    pub fn on_delete(
        &self,
        mut callback: impl FnMut(&<Row::Module as SpacetimeModule>::EventContext, &Row) + Send + 'static,
    ) -> CallbackId {
        let callback_id = CallbackId::get_next();
        self.queue_mutation(PendingMutation::AddDeleteCallback {
            table: self.table,
            callback: Box::new(move |ctx, row| {
                let row = row.downcast_ref::<Row>().unwrap();
                callback(ctx, row);
            }),
            callback_id,
        });
        callback_id
    }

    /// Called by the autogenerated implementation of the [`crate::Table`] method of the same name.
    pub fn remove_on_delete(&self, callback: CallbackId) {
        self.queue_mutation(PendingMutation::RemoveDeleteCallback {
            table: self.table,
            callback_id: callback,
        });
    }

    /// Called by the autogenerated implementation of the [`crate::TableWithPrimaryKey`] method of the same name.
    pub fn on_update(
        &self,
        mut callback: impl FnMut(&<Row::Module as SpacetimeModule>::EventContext, &Row, &Row) + Send + 'static,
    ) -> CallbackId {
        let callback_id = CallbackId::get_next();
        self.queue_mutation(PendingMutation::AddUpdateCallback {
            table: self.table,
            callback: Box::new(move |ctx, old, new| {
                let old = old.downcast_ref::<Row>().unwrap();
                let new = new.downcast_ref::<Row>().unwrap();
                callback(ctx, old, new);
            }),
            callback_id,
        });
        callback_id
    }

    /// Called by the autogenerated implementation of the [`crate::TableWithPrimaryKey`] method of the same name.
    pub fn remove_on_update(&self, callback: CallbackId) {
        self.queue_mutation(PendingMutation::RemoveUpdateCallback {
            table: self.table,
            callback_id: callback,
        });
    }

    /// Called by autogenerated unique index access methods.
    pub fn get_unique_constraint<Col>(
        &self,
        _constraint_name: &'static str,
        get_unique_field: fn(&Row) -> &Col,
    ) -> UniqueConstraint<Row, Col> {
        UniqueConstraint {
            table: Arc::clone(&self.table_view),
            get_unique_field,
        }
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
        }));
        let cache = Arc::new(StdMutex::new(Arc::new(ClientCache::default())));
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

    /// Set the name or address of the remote module.
    pub fn with_module_name(mut self, name_or_address: impl ToString) -> Self {
        self.module_name = Some(name_or_address.to_string());
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
    InitialSubscription { db_update: M::DbUpdate, sub_id: u32 },
    TransactionUpdate(Event<M::Reducer>, Option<M::DbUpdate>),
    IdentityToken(Identity, String, Address),
    Error(anyhow::Error),
}

fn spawn_parse_loop<M: SpacetimeModule>(
    raw_message_recv: mpsc::UnboundedReceiver<ws::ServerMessage>,
    handle: &runtime::Handle,
) -> (tokio::task::JoinHandle<()>, mpsc::UnboundedReceiver<ParsedMessage<M>>) {
    let (parsed_message_send, parsed_message_recv) = mpsc::unbounded();
    let handle = handle.spawn(parse_loop(raw_message_recv, parsed_message_send));
    (handle, parsed_message_recv)
}

/// A loop which reads raw WS messages from `recv`, parses them into domain types,
/// and pushes the [`ParsedMessage`]s into `send`.
async fn parse_loop<M: SpacetimeModule>(
    mut recv: mpsc::UnboundedReceiver<ws::ServerMessage>,
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
            ws::ServerMessage::IdentityToken(ws::IdentityToken {
                identity,
                token,
                address,
            }) => ParsedMessage::IdentityToken(identity, token, address),
            ws::ServerMessage::OneOffQueryResponse(_) => {
                unreachable!("The Rust SDK does not implement one-off queries")
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
        queries: Vec<String>,
        // TODO: replace `queries` with query_sql: String,
        sub_id: u32,
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
