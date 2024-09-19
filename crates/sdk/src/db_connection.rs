use crate::{
    callbacks::{CallbackId, DbCallbacks, ReducerCallbacks},
    client_cache::{ClientCache, ClientCacheView, TableCache, UniqueConstraint},
    spacetime_module::{DbConnection, DbUpdate, EventContext, InModule, SpacetimeModule},
    subscription::SubscriptionManager,
    websocket::WsConnection,
    ws_messages as ws, Event, ReducerEvent, Status,
};
use anyhow::{bail, Context, Result};
use futures::StreamExt;
use futures_channel::mpsc;
use http::Uri;
use spacetimedb_lib::{bsatn, de::Deserialize, ser::Serialize, Address, Identity};
use std::{
    any::Any,
    marker::PhantomData,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tokio::runtime::{self, Runtime};

pub(crate) type SharedCell<T> = Arc<Mutex<T>>;

pub struct DbContextImpl<M: SpacetimeModule> {
    runtime: runtime::Handle,
    pub(crate) inner: SharedCell<DbContextImplInner<M>>,
    cache: SharedCell<ClientCacheView<M>>,
    recv: SharedCell<mpsc::UnboundedReceiver<ParsedMessage<M>>>,
    pub(crate) pending_mutations_send: mpsc::UnboundedSender<PendingMutation<M>>,
    pending_mutations_recv: SharedCell<mpsc::UnboundedReceiver<PendingMutation<M>>>,
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
        }
    }
}

impl<M: SpacetimeModule> DbContextImpl<M> {
    fn process_message(&self, msg: ParsedMessage<M>) -> Result<()> {
        let res = match msg {
            ParsedMessage::Error(e) => Err(e),
            ParsedMessage::IdentityToken(identity, token, addr) => {
                let mut inner = self.inner.lock().unwrap();
                assert_eq!(inner.address, addr);
                if let Some(prev_identity) = inner.identity {
                    assert_eq!(prev_identity, identity);
                }
                inner.identity = Some(identity);
                if let Some(on_connect) = inner.on_connect.take() {
                    on_connect(identity, &token);
                }
                Ok(())
            }
            ParsedMessage::InitialSubscription { db_update, sub_id } => {
                let prev_cache_view = Arc::clone(&*self.cache.lock().unwrap());
                let mut new_cache = ClientCache::clone(&*prev_cache_view);
                db_update.apply_to_client_cache(&mut new_cache);
                let new_cache_view = Arc::new(new_cache);
                *self.cache.lock().unwrap() = new_cache_view;
                let event_ctx = self.make_event_ctx(Event::SubscribeApplied);
                let mut inner = self.inner.lock().unwrap();
                inner.subscriptions.subscription_applied(&event_ctx, sub_id);
                db_update.invoke_row_callbacks(&event_ctx, &mut inner.db_callbacks);
                Ok(())
            }
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

    fn make_event_ctx(&self, event: Event<M::Reducer>) -> M::EventContext {
        let imp = self.clone();
        M::EventContext::new(imp, event)
    }

    /// To avoid deadlocks during callbacks, we make all mutations to subscription- and callback-managing structurs
    /// strictly after running those callbacks, stashing them in a channel during the actual callback runs.
    fn apply_pending_mutations(&self) {
        while let Ok(Some(pending_mutation)) = self.pending_mutations_recv.lock().unwrap().try_next() {
            self.apply_mutation(pending_mutation);
        }
    }

    fn apply_mutation(&self, mutation: PendingMutation<M>) {
        match mutation {
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
                    .unbounded_send(ws::ClientMessage::Subscribe(ws::Subscribe {
                        query_strings: queries,
                        request_id: 0,
                    }))
                    .expect("Unable to send subscribe message: WS sender loop has dropped its recv channel");
            }
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
                    .unbounded_send(msg)
                    .expect("Unable to send reducer call message: WS sender loop has dropped its recv channel");
            }
            PendingMutation::Disconnect => todo!(),
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
        }
    }

    pub fn advance_one_message(&self) -> Result<bool> {
        self.apply_pending_mutations();
        // Deranged behavior: mpsc's `try_next` returns `Ok(None)` when the channel is closed,
        // and `Err(_)` when the channel is open and waiting. This seems exactly backwards.
        let res = match self.recv.lock().unwrap().try_next() {
            // TODO: downcastable error that combinators can interpret.
            // TODO: call `on_disconnect`.
            Ok(None) => Err(anyhow::anyhow!("`advance_one_message` on closed `DbConnection`")),
            Err(_) => Ok(false),
            Ok(Some(msg)) => self.process_message(msg).map(|_| true),
        };
        self.apply_pending_mutations();
        res
    }

    async fn get_message(&self) -> Message<M> {
        // Holding these locks across the below await can only cause a deadlock if
        // there are multiple parallel callers of `advance_one_message` or its siblings.
        // We call this out as an incorrect and unsupported thing to do.
        #![allow(clippy::await_holding_lock)]

        let mut pending_mutations = self.pending_mutations_recv.lock().unwrap();
        let mut recv = self.recv.lock().unwrap();

        tokio::select! {
            pending_mutation = pending_mutations.next() => Message::Local(pending_mutation.unwrap()),
            incoming_message = recv.next() => Message::Ws(incoming_message),
        }
    }

    pub fn advance_one_message_blocking(&self) -> Result<()> {
        match self.runtime.block_on(self.get_message()) {
            Message::Local(pending) => {
                self.apply_mutation(pending);
                Ok(())
            }
            // TODO: downcastable error that combinators can interpret.
            // TODO: call `on_disconnect`.
            Message::Ws(None) => Err(anyhow::anyhow!("`advance_one_message` on closed `DbConnection`")),
            Message::Ws(Some(msg)) => self.process_message(msg),
        }
    }

    pub async fn advance_one_message_async(&self) -> Result<()> {
        match self.get_message().await {
            Message::Local(pending) => {
                self.apply_mutation(pending);
                Ok(())
            }
            // TODO: downcastable error that combinators can interpret.
            // TODO: call `on_disconnect`.
            Message::Ws(None) => Err(anyhow::anyhow!("`advance_one_message` on closed `DbConnection`")),
            Message::Ws(Some(msg)) => self.process_message(msg),
        }
    }

    pub fn frame_tick(&self) -> Result<()> {
        while self.advance_one_message()? {}
        Ok(())
    }

    pub fn run_threaded(&self) -> std::thread::JoinHandle<()> {
        let this = self.clone();
        std::thread::spawn(move || {
            loop {
                match this.advance_one_message_blocking() {
                    Ok(()) => (),
                    // TODO: Err(e) if error_is_normal_disconnect(&e) => return,
                    Err(e) => panic!("{e:?}"),
                }
            }
        })
    }

    pub async fn run_async(&self) -> Result<()> {
        let this = self.clone();
        loop {
            match this.advance_one_message_async().await {
                Ok(()) => (),
                // TODO: Err(e) if error_is_normal_disconnect(&e) => return,
                Err(e) => return Err(e),
            }
        }
    }

    pub fn is_active(&self) -> bool {
        todo!()
    }

    pub fn disconnect(&self) -> Result<()> {
        if !self.is_active() {
            bail!("Already disconnected in call to `DbContext::disconnect`");
        }
        self.pending_mutations_send
            .unbounded_send(PendingMutation::Disconnect)
            .unwrap();
        Ok(())
    }

    pub fn subscribe() {
        todo!()
    }

    /// Add a [`PendingMutation`] to the `pending_mutations` queue,
    /// to be processed during the next call to [`Self::apply_pending_mutations`].
    ///
    /// This is used to defer operations which would otherwise need to hold a lock on `self.inner`,
    /// as otherwise running those operations within a callback would deadlock.
    fn queue_mutation(&self, mutation: PendingMutation<M>) {
        self.pending_mutations_send.unbounded_send(mutation).unwrap();
    }

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

    pub fn call_reducer<Args: Serialize + InModule<Module = M>>(
        &self,
        reducer_name: &'static str,
        args: Args,
    ) -> Result<()> {
        log::info!("call_reducer: {reducer_name:?}");
        let args_bsatn = bsatn::to_vec(&args)
            .with_context(|| format!("Failed to BSATN serialize arguments for reducer {reducer_name}"))?;
        self.queue_mutation(PendingMutation::CallReducer {
            reducer: reducer_name,
            args_bsatn,
        });
        log::info!("call_reducer: done");
        Ok(())
    }

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

    pub fn remove_on_reducer<Args: InModule<Module = M>>(&self, reducer_name: &'static str, callback: CallbackId) {
        self.queue_mutation(PendingMutation::RemoveReducerCallback {
            reducer: reducer_name,
            callback_id: callback,
        });
    }
}

pub(crate) struct DbContextImplInner<M: SpacetimeModule> {
    /// `Some` if not within the context of an outer runtime. The `Runtime` must
    /// then live as long as `Self`.
    #[allow(unused)]
    runtime: Option<Runtime>,

    /// None if not yet connected.
    send_chan: mpsc::UnboundedSender<ws::ClientMessage>,

    db_callbacks: DbCallbacks<M>,
    reducer_callbacks: ReducerCallbacks<M>,
    pub(crate) subscriptions: SubscriptionManager<M>,

    identity: Option<Identity>,
    address: Address,

    on_connect: Option<Box<dyn FnOnce(Identity, &str) + Send + Sync + 'static>>,
    #[allow(unused)]
    // TODO: Make use of this to handle `ParsedMessage::Error`?
    on_connect_error: Option<Box<dyn FnOnce(anyhow::Error) + Send + Sync + 'static>>,
    #[allow(unused)]
    // TODO: implement disconnection logic.
    on_disconnect: Option<Box<dyn FnOnce(&M::DbConnection, Option<anyhow::Error>) + Send + Sync + 'static>>,

    _module: PhantomData<M>,
}

pub struct TableHandle<Row: InModule> {
    /// May be `None` if there are no rows in the table cache.
    table_view: Arc<TableCache<Row>>,
    pending_mutations: mpsc::UnboundedSender<PendingMutation<Row::Module>>,
    table: &'static str,
}

impl<Row: InModule + Send + Sync + Clone + 'static> TableHandle<Row> {
    pub fn count(&self) -> u64 {
        self.table_view.entries.len() as u64
    }

    pub fn iter(&self) -> impl Iterator<Item = Row> + '_ {
        self.table_view.entries.values().cloned()
    }

    /// See [`DbContextImpl::queue_mutation`].
    fn queue_mutation(&self, mutation: PendingMutation<Row::Module>) {
        self.pending_mutations.unbounded_send(mutation).unwrap();
    }

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

    pub fn remove_on_insert(&self, callback: CallbackId) {
        self.queue_mutation(PendingMutation::RemoveInsertCallback {
            table: self.table,
            callback_id: callback,
        });
    }

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

    pub fn remove_on_delete(&self, callback: CallbackId) {
        self.queue_mutation(PendingMutation::RemoveDeleteCallback {
            table: self.table,
            callback_id: callback,
        });
    }

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

    pub fn remove_on_update(&self, callback: CallbackId) {
        self.queue_mutation(PendingMutation::RemoveUpdateCallback {
            table: self.table,
            callback_id: callback,
        });
    }

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

pub struct DbConnectionBuilder<M: SpacetimeModule> {
    uri: Option<Uri>,

    module_name: Option<String>,

    credentials: Option<(Identity, String)>,

    on_connect: Option<Box<dyn FnOnce(Identity, &str) + Send + Sync + 'static>>,
    on_connect_error: Option<Box<dyn FnOnce(anyhow::Error) + Send + Sync + 'static>>,
    on_disconnect: Option<Box<dyn FnOnce(&M::DbConnection, Option<anyhow::Error>) + Send + Sync + 'static>>,
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

    pub fn build(self) -> Result<M::DbConnection> {
        let imp = self.build_impl()?;
        Ok(<M::DbConnection as DbConnection>::new(imp))
    }

    fn build_impl(self) -> Result<DbContextImpl<M>> {
        let (runtime, handle) = enter_or_create_runtime()?;
        let db_callbacks = DbCallbacks::default();
        let reducer_callbacks = ReducerCallbacks::default();
        let client_address = Address::from_byte_array(rand::random());

        let ws_connection = tokio::task::block_in_place(|| {
            handle.block_on(WsConnection::connect(
                self.uri.unwrap(),
                self.module_name.as_ref().unwrap(),
                self.credentials.as_ref(),
                client_address,
            ))
        })?;

        let (_websocket_loop_handle, raw_msg_recv, raw_msg_send) = ws_connection.spawn_message_loop(&handle);
        let (_parse_loop_handle, parsed_recv_chan) = spawn_parse_loop::<M>(raw_msg_recv, &handle);

        let inner = Arc::new(Mutex::new(DbContextImplInner {
            runtime,

            send_chan: raw_msg_send,
            db_callbacks,
            reducer_callbacks,
            subscriptions: SubscriptionManager::default(),

            identity: self.credentials.as_ref().map(|creds| creds.0),
            address: client_address,

            on_connect: self.on_connect,
            on_connect_error: self.on_connect_error,
            on_disconnect: self.on_disconnect,

            _module: PhantomData,
        }));
        let cache = Arc::new(Mutex::new(Arc::new(ClientCache::default())));
        let (pending_mutations_send, pending_mutations_recv) = mpsc::unbounded();
        let ctx_imp = DbContextImpl {
            runtime: handle,
            inner,
            cache,
            recv: Arc::new(Mutex::new(parsed_recv_chan)),
            pending_mutations_send,
            pending_mutations_recv: Arc::new(Mutex::new(pending_mutations_recv)),
        };

        Ok(ctx_imp)
    }

    pub fn with_uri<E: std::fmt::Debug>(mut self, uri: impl TryInto<Uri, Error = E>) -> Self {
        let uri = uri.try_into().expect("Unable to parse supplied URI");
        self.uri = Some(uri);
        self
    }

    pub fn with_module_name(mut self, name_or_address: impl ToString) -> Self {
        self.module_name = Some(name_or_address.to_string());
        self
    }

    pub fn with_credentials(mut self, credentials: Option<(Identity, String)>) -> Self {
        self.credentials = credentials;
        self
    }

    pub fn on_connect(mut self, callback: impl FnOnce(Identity, &str) + Send + Sync + 'static) -> Self {
        if self.on_connect.is_some() {
            panic!(
                "DbConnectionBuilder can only register a single `on_connect` callback.

Instead of registering multiple `on_connect` callbacks, register a single callback which does multiple operations."
            );
        }

        self.on_connect = Some(Box::new(callback));
        self
    }

    pub fn on_connect_error(mut self, callback: impl FnOnce(anyhow::Error) + Send + Sync + 'static) -> Self {
        if self.on_connect_error.is_some() {
            panic!(
                "DbConnectionBuilder can only register a single `on_connect_error` callback.

Instead of registering multiple `on_connect_error` callbacks, register a single callback which does multiple operations."
            );
        }

        self.on_connect_error = Some(Box::new(callback));
        self
    }

    pub fn on_disconnect(
        mut self,
        callback: impl FnOnce(&M::DbConnection, Option<anyhow::Error>) + Send + Sync + 'static,
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
        on_applied: Option<Box<dyn FnOnce(&M::EventContext) + Send + 'static>>,
        on_error: Option<Box<dyn FnOnce(&M::EventContext) + Send + 'static>>,
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
        callback: Box<dyn FnMut(&M::EventContext, &dyn Any) + Send + 'static>,
    },
    RemoveInsertCallback {
        table: &'static str,
        callback_id: CallbackId,
    },
    AddDeleteCallback {
        table: &'static str,
        callback_id: CallbackId,
        callback: Box<dyn FnMut(&M::EventContext, &dyn Any) + Send + 'static>,
    },
    RemoveDeleteCallback {
        table: &'static str,
        callback_id: CallbackId,
    },
    AddUpdateCallback {
        table: &'static str,
        callback_id: CallbackId,
        callback: Box<dyn FnMut(&M::EventContext, &dyn Any, &dyn Any) + Send + 'static>,
    },
    RemoveUpdateCallback {
        table: &'static str,
        callback_id: CallbackId,
    },
    AddReducerCallback {
        reducer: &'static str,
        callback_id: CallbackId,
        callback: Box<dyn FnMut(&M::EventContext, &dyn Any) + Send + 'static>,
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