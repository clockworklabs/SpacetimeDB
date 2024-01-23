use crate::callbacks::{
    CredentialStore, DbCallbacks, DisconnectCallbacks, ReducerCallbacks, SubscriptionAppliedCallbacks,
};
use crate::client_api_messages;
use crate::client_cache::{ClientCache, ClientCacheView, RowCallbackReminders};
use crate::global_connection::CLIENT_CACHE;
use crate::identity::Credentials;
use crate::reducer::{AnyReducerEvent, Reducer};
use crate::spacetime_module::SpacetimeModule;
use crate::websocket::DbConnection;
use anyhow::{Context, Result};
use futures::stream::StreamExt;
use futures_channel::mpsc;
use spacetimedb_sats::bsatn;
use std::sync::{Arc, Mutex};
use tokio::runtime::{self, Builder, Runtime};
use tokio::task::JoinHandle;

/// A thread-safe mutable place that can be shared by multiple referents.
type SharedCell<T> = Arc<Mutex<T>>;

pub struct BackgroundDbConnection {
    /// `Some` if not within the context of an outer runtime. The `Runtime` must
    /// then live as long as `Self`.
    #[allow(unused)]
    runtime: Option<Runtime>,

    handle: runtime::Handle,
    /// None if not yet connected.
    send_chan: Option<mpsc::UnboundedSender<client_api_messages::Message>>,
    #[allow(unused)]
    /// None if not yet connected.
    websocket_loop_handle: Option<JoinHandle<()>>,
    #[allow(unused)]
    /// None if not yet connected.
    recv_handle: Option<JoinHandle<()>>,
    #[allow(unused)]
    pub(crate) credentials: SharedCell<CredentialStore>,

    /// The most recent state of the `ClientCache`, kept in a shared cell
    /// so that the `receiver_loop` can update it, and non-callback table accesses
    /// can observe it via `global_connection::current_or_global_state`.
    ///
    /// If you expand these type aliases, you get `Arc<Mutex<Arc<ClientCache>>>`,
    /// which looks somewhat strange. The type aliases are intended to make clear
    /// the purpose of the two layers of refcounting:
    ///
    /// The outer layer, around the `Mutex`, allows for a shared mutable cell
    /// by which multiple concurrent workers can communicate the most recent state.
    ///
    /// The inner layer, around the `ClientCache`, allows those workers to
    /// cheaply extract a snapshot of the `ClientCache`
    /// without holding a lock for the lifetime of that snapshot,
    /// and without changes to the state invalidating or altering the snapshot.
    ///
    /// None if not yet connected.
    pub(crate) client_cache: SharedCell<Option<ClientCacheView>>,

    pub(crate) db_callbacks: SharedCell<DbCallbacks>,
    pub(crate) reducer_callbacks: SharedCell<ReducerCallbacks>,
    pub(crate) subscription_callbacks: SharedCell<SubscriptionAppliedCallbacks>,
    pub(crate) disconnect_callbacks: SharedCell<DisconnectCallbacks>,
}

// When called from within an async context, return a handle to it (and no
// `Runtime`), otherwise create a fresh `Runtime` and return it along with a
// handle to it.
fn enter_or_create_runtime() -> Result<(Option<Runtime>, runtime::Handle)> {
    match runtime::Handle::try_current() {
        Err(e) if e.is_missing_context() => {
            let rt = Builder::new_multi_thread()
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

fn process_table_update(
    update: client_api_messages::TableUpdate,
    client_cache: &mut ClientCache,
    callback_reminders: &mut RowCallbackReminders,
) {
    client_cache.handle_table_update(callback_reminders, update);
}

fn process_subscription_update_for_new_subscribed_set(
    msg: client_api_messages::SubscriptionUpdate,
    client_cache: &mut ClientCache,
    callback_reminders: &mut RowCallbackReminders,
) {
    for update in msg.table_updates {
        client_cache.handle_table_reinitialize_for_new_subscribed_set(callback_reminders, update);
    }
}

fn process_subscription_update_for_transaction_update(
    msg: client_api_messages::SubscriptionUpdate,
    client_cache: &mut ClientCache,
    callback_reminders: &mut RowCallbackReminders,
) {
    for update in msg.table_updates {
        process_table_update(update, client_cache, callback_reminders);
    }
}

fn process_event(
    msg: client_api_messages::Event,
    reducer_callbacks: &mut ReducerCallbacks,
    state: ClientCacheView,
) -> Option<Arc<AnyReducerEvent>> {
    reducer_callbacks.handle_event(msg, state)
}

/// Advance the client cache state by `update`
/// starting from the state in `client_cache`,
/// then store the new state back into `client_cache` and return it.
///
/// The existing `ClientCacheView` in `client_cache` is not mutated,
/// so handles on it held in other places (e.g. by callback workers)
/// remain valid. That is, these workers store their own snapshots
/// of the `ClientCache`, and `update_client_cache` does not alter those snapshots.
///
/// The lock on `client_cache` is held
/// for the duration of the `update` function's invocation,
/// in order to maintain a strict sequence between `update_client_cache` calls.
/// Otherwise, we'd be in software transactional memory territory,
/// and would have to use compare-and-swap
/// to check whether the cache state had changed during `update`,
/// then either retry or merge.
///
/// When handling a message which updates the client cache,
/// i.e. a `SubscriptionUpdate` or `TransactionUpdate`,
/// the message handler will apply the changes within an `update_client_cache` call,
/// then invoke callbacks on the resulting `ClientCacheView`.
fn update_client_cache(
    client_cache: &Mutex<Option<ClientCacheView>>,
    update: impl FnOnce(&mut ClientCache),
) -> ClientCacheView {
    let mut cache_lock = client_cache.lock().expect("ClientCache Mutex is poisoned");
    // Make a new state starting from the one in `cache_lock`.
    // `new_state` is not yet shared, and so can be mutated.
    let mut new_state = ClientCache::clone(cache_lock.as_ref().unwrap());
    // Advance `new_state` to hold any changes.
    update(&mut new_state);
    // Make `new_state` shared, and store it back into `cache_lock`.
    *cache_lock = Some(Arc::new(new_state));
    // Return the new state.
    Option::clone(&cache_lock).unwrap()
}

fn process_transaction_update(
    client_api_messages::TransactionUpdate {
        subscription_update,
        event,
    }: client_api_messages::TransactionUpdate,
    client_cache: &Mutex<Option<ClientCacheView>>,
    db_callbacks: &Mutex<DbCallbacks>,
    reducer_callbacks: &Mutex<ReducerCallbacks>,
) {
    // Process the updated tables in the `subscription_update`.
    if let Some(update) = subscription_update {
        let mut callback_reminders = RowCallbackReminders::new_for_subscription_update(&update);
        let new_state = update_client_cache(client_cache, |client_cache| {
            process_subscription_update_for_transaction_update(update, client_cache, &mut callback_reminders);
        });

        let mut db_callbacks_lock = db_callbacks.lock().expect("DbCallbacks Mutex is poisoned");

        if let Some(event) = event {
            let mut reducer_lock = reducer_callbacks.lock().expect("ReducerCallbacks Mutex is poisoned");
            let event = process_event(event, &mut reducer_lock, new_state.clone());
            new_state.invoke_row_callbacks(&mut callback_reminders, &mut db_callbacks_lock, event);
        } else {
            log::error!("Received TransactionUpdate with no Event");
            new_state.invoke_row_callbacks(&mut callback_reminders, &mut db_callbacks_lock, None);
        }
    } else {
        log::error!("Received TransactionUpdate with no SubscriptionUpdate");
    }
}

// This function's future will be run in the background with `Runtime::spawn`, so the
// future must be `'static`. As a result, it must own (shared pointers to) the
// `ClientCache`, `ReducerCallbacks` and `Credentials`, rather than references.
async fn receiver_loop(
    mut recv: mpsc::UnboundedReceiver<client_api_messages::Message>,
    client_cache: SharedCell<Option<ClientCacheView>>,
    db_callbacks: SharedCell<DbCallbacks>,
    reducer_callbacks: SharedCell<ReducerCallbacks>,
    credentials: SharedCell<CredentialStore>,
    subscription_callbacks: SharedCell<SubscriptionAppliedCallbacks>,
    disconnect_callbacks: SharedCell<DisconnectCallbacks>,
) {
    while let Some(msg) = recv.next().await {
        match msg {
            client_api_messages::Message { r#type: None } => (),
            client_api_messages::Message {
                r#type: Some(client_api_messages::message::Type::SubscriptionUpdate(update)),
            } => {
                log::info!("Message SubscriptionUpdate");
                let mut callback_reminders = RowCallbackReminders::new_for_subscription_update(&update);
                let new_state = update_client_cache(&client_cache, |client_cache| {
                    process_subscription_update_for_new_subscribed_set(update, client_cache, &mut callback_reminders);
                });

                subscription_callbacks
                    .lock()
                    .expect("SubscriptionAppliedCallbacks Mutex is poisoned")
                    .handle_subscription_applied(new_state.clone());

                let mut db_callbacks_lock = db_callbacks.lock().expect("DbCallbacks Mutex is poisoned");
                new_state.invoke_row_callbacks(&mut callback_reminders, &mut db_callbacks_lock, None);
            }
            client_api_messages::Message {
                r#type: Some(client_api_messages::message::Type::TransactionUpdate(transaction_update)),
            } => {
                log::trace!("Message TransactionUpdate");

                process_transaction_update(transaction_update, &client_cache, &db_callbacks, &reducer_callbacks);
            }
            client_api_messages::Message {
                r#type: Some(client_api_messages::message::Type::IdentityToken(ident)),
            } => {
                log::trace!("Message IdentityToken");
                let state = Option::clone(&client_cache.lock().expect("ClientCache Mutex is poisoned")).unwrap();
                let mut credentials_lock = credentials.lock().expect("Credentials Mutex is poisoned");
                credentials_lock.handle_identity_token(ident, state);
            }
            other => log::info!("Unknown message: {:?}", other),
        }
    }
    let final_state = client_cache.lock().expect("ClientCache Mutex is poisoned");
    let final_state = ClientCacheView::clone(final_state.as_ref().unwrap());
    disconnect_callbacks
        .lock()
        .expect("DisconnectCallbacks Mutex is poisoned")
        .handle_disconnect(final_state);
}

impl BackgroundDbConnection {
    /// Construct a partially-initialized `BackgroundDbConnection`
    /// which can register callbacks, but not handle events.
    ///
    /// The `BackgroundDbConnection` will be fully initialized upon calling `connect`.
    pub(crate) fn unconnected() -> Result<Self> {
        let (runtime, handle) = enter_or_create_runtime()?;
        let db_callbacks = Arc::new(Mutex::new(DbCallbacks::new(handle.clone())));
        let reducer_callbacks = Arc::new(Mutex::new(ReducerCallbacks::without_handle_event(handle.clone())));
        let credentials = Arc::new(Mutex::new(CredentialStore::without_credentials(&handle)));
        let subscription_callbacks = Arc::new(Mutex::new(SubscriptionAppliedCallbacks::new(&handle)));
        let disconnect_callbacks = Arc::new(Mutex::new(DisconnectCallbacks::new(&handle)));

        Ok(BackgroundDbConnection {
            runtime,
            handle,
            send_chan: None,
            websocket_loop_handle: None,
            recv_handle: None,
            credentials,
            client_cache: Arc::clone(&CLIENT_CACHE),
            db_callbacks,
            reducer_callbacks,
            subscription_callbacks,
            disconnect_callbacks,
        })
    }

    fn spawn_receiver(
        &self,
        recv: mpsc::UnboundedReceiver<client_api_messages::Message>,
        client_cache: SharedCell<Option<ClientCacheView>>,
    ) -> JoinHandle<()> {
        self.handle.spawn(receiver_loop(
            recv,
            client_cache,
            self.db_callbacks.clone(),
            self.reducer_callbacks.clone(),
            self.credentials.clone(),
            self.subscription_callbacks.clone(),
            self.disconnect_callbacks.clone(),
        ))
    }

    /// Connect to a database named `db_name` accessible over the internet at the URI `spacetimedb_uri`.
    ///
    /// If `credentials` are supplied, they will be passed to the new connection to
    /// identify and authenticate the user. Otherwise, a set of `Credentials` will be
    /// generated by the server.
    ///
    /// `handle_table_update`, `handle_resubscribe` and `handle_function_call` are
    /// functions autogenerated by the SpacetimeDB CLI in `mod.rs` which dispatch on various
    /// messages from the server in order to deserialize incoming rows. The CLI will
    /// generate and export a function `connect` from the `mod.rs` which wraps this
    /// function and passes these arguments automatically.
    ///
    /// Users should not call `BackgroundDbConnection::connect` directly;
    /// instead, call the `connect` function generated by the SpacetimeDB CLI.
    pub fn connect<IntoUri>(
        &mut self,
        spacetimedb_uri: IntoUri,
        db_name: &str,
        credentials: Option<Credentials>,
        module: Arc<dyn SpacetimeModule>,
    ) -> Result<()>
    where
        IntoUri: TryInto<http::Uri>,
        <IntoUri as TryInto<http::Uri>>::Error: std::error::Error + Send + Sync + 'static,
    {
        // Disconnect to prevent any outstanding messages from contending for unique resources,
        // i.e. the reducer callbacks, credential store and client cache.
        self.disconnect();

        // Hold all internal locks for the duration of this method,
        // to prevent races if the connection starts receiving messages
        // before this method returns.
        let mut reducer_callbacks_lock = self
            .reducer_callbacks
            .lock()
            .expect("ReducerCallbacks Mutex is poisoned");
        let mut credentials_lock = self.credentials.lock().expect("CredentialStore Mutex is poisoned");
        let mut client_cache_lock = self.client_cache.lock().expect("ClientCache Mutex is poisoned");

        // Specialize the reducer callbacks for this module.
        // Registering callbacks doesn't require the module, but firing them does,
        // in order to parse arguments with appropriate types.
        reducer_callbacks_lock.set_module(module.clone());

        // If credentials were passed to connect, store them in the credential store.
        // If not, unset any stale credentials in the credential store.
        credentials_lock.maybe_set_credentials(credentials.clone());

        let client_address = credentials_lock.get_or_init_address();

        // Construct a new client cache specialized for this module.
        // The client cache needs to know the module
        // in order to parse rows and issue callbacks with appropriate types.
        let client_cache = Arc::new(ClientCache::new(module.clone()));
        *client_cache_lock = Some(client_cache);

        // `block_in_place` is required here, as tokio won't allow us to call
        // `block_on` if it would block the current thread of an outer runtime
        let connection = tokio::task::block_in_place(|| {
            self.handle.block_on(DbConnection::connect(
                spacetimedb_uri,
                db_name,
                credentials.as_ref(),
                client_address,
            ))
        })?;

        let (websocket_loop_handle, recv_chan, send_chan) = connection.spawn_message_loop(&self.handle);
        let recv_handle = self.spawn_receiver(recv_chan, self.client_cache.clone());

        self.send_chan = Some(send_chan);
        self.websocket_loop_handle = Some(websocket_loop_handle);
        self.recv_handle = Some(recv_handle);

        Ok(())
    }

    pub fn disconnect(&mut self) {
        self.send_chan = None;

        // `block_in_place` over `block_on` allows us to wait for a future to complete
        // regardless of whether we're currently running in an `async` task or not.
        if let Some(h) = self.websocket_loop_handle.take() {
            let _ = tokio::task::block_in_place(|| self.handle.block_on(h));
        }
        if let Some(h) = self.recv_handle.take() {
            let _ = tokio::task::block_in_place(|| self.handle.block_on(h));
        }
    }

    fn send_message(&self, message: client_api_messages::Message) -> Result<()> {
        self.send_chan
            .as_ref()
            .context("Cannot send message before connecting")?
            .unbounded_send(message)
            .context("Sending message to remote DB")
    }

    pub(crate) fn subscribe(&self, queries: &[&str]) -> Result<()> {
        self.subscribe_owned(queries.iter().map(|&s| s.into()).collect())
    }

    pub(crate) fn subscribe_owned(&self, queries: Vec<String>) -> Result<()> {
        self.send_message(client_api_messages::Message {
            r#type: Some(client_api_messages::message::Type::Subscribe(
                client_api_messages::Subscribe { query_strings: queries },
            )),
        })
        .with_context(|| "Subscribing to new queries")
    }

    pub(crate) fn invoke_reducer<R: Reducer>(&self, reducer: R) -> Result<()> {
        self.send_message(client_api_messages::Message {
            r#type: Some(client_api_messages::message::Type::FunctionCall(
                client_api_messages::FunctionCall {
                    reducer: R::REDUCER_NAME.to_string(),
                    arg_bytes: bsatn::to_vec(&reducer).expect("Serializing reducer failed"),
                },
            )),
        })
        .with_context(|| format!("Invoking reducer {}", R::REDUCER_NAME))
    }
}
