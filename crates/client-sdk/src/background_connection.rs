use crate::callbacks::{CredentialStore, DbCallbacks, ReducerCallbacks};
use crate::client_api_messages;
use crate::client_cache::{ClientCache, ClientCacheView, RowCallbackReminders};
use crate::identity::Credentials;
use crate::reducer::Reducer;
use crate::websocket::DbConnection;
use anyhow::Result;
use futures::stream::StreamExt;
use futures_channel::mpsc;
use spacetimedb_sats::bsatn;
use std::sync::{Arc, Mutex};
use tokio::{
    runtime::{Builder, Runtime},
    task::JoinHandle,
};

/// A thread-safe mutable place that can be shared by multiple referents.
type SharedCell<T> = Arc<Mutex<T>>;

pub struct BackgroundDbConnection {
    #[allow(unused)]
    runtime: Arc<Runtime>,
    send_chan: mpsc::UnboundedSender<client_api_messages::Message>,
    #[allow(unused)]
    websocket_loop_handle: JoinHandle<()>,
    #[allow(unused)]
    recv_handle: JoinHandle<()>,
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
    /// cheaply extract a reference to the `ClientCache`
    /// without holding a lock for the lifetime of that reference,
    /// and without changes to the state invalidating the reference.
    pub(crate) client_cache: SharedCell<ClientCacheView>,

    pub(crate) db_callbacks: SharedCell<DbCallbacks>,
    pub(crate) reducer_callbacks: SharedCell<ReducerCallbacks>,
}

fn make_runtime() -> Result<Runtime> {
    Ok(Builder::new_multi_thread()
        .enable_all()
        .worker_threads(1)
        .thread_name("spacetimedb-background-connection")
        .build()?)
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

fn process_event(msg: client_api_messages::Event, reducer_callbacks: &mut ReducerCallbacks, state: ClientCacheView) {
    reducer_callbacks.handle_event(msg, state);
}

/// Advance the client cache state by `update`
/// starting from the state in `client_cache`,
/// then store the new state back into `client_cache` and return it.
///
/// The existing `ClientCacheView` in `client_cache` is not mutated,
/// so handles on it held in other places (e.g. by callback workers)
/// remain valid.
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
    client_cache: &Mutex<ClientCacheView>,
    update: impl FnOnce(&mut ClientCache),
) -> ClientCacheView {
    let mut cache_lock = client_cache.lock().expect("ClientCache Mutex is poisoned");
    // Make a new state starting from the one in `cache_lock`.
    // `new_state` is not yet shared, and so can be mutated.
    let mut new_state = ClientCache::clone(&cache_lock);
    // Advance `new_state` to hold any changes.
    update(&mut new_state);
    // Make `new_state` shared, and store it back into `cache_lock`.
    *cache_lock = Arc::new(new_state);
    // Return the new state.
    Arc::clone(&cache_lock)
}

fn process_transaction_update(
    client_api_messages::TransactionUpdate {
        subscription_update,
        event,
    }: client_api_messages::TransactionUpdate,
    client_cache: &Mutex<ClientCacheView>,
    db_callbacks: &Mutex<DbCallbacks>,
    reducer_callbacks: &Mutex<ReducerCallbacks>,
) {
    // TODO: should we have some third kind of callback that takes both a
    //       `Reducer` and a `TableType` so clients can observe all of a `TransactionUpdate`?

    // TODO: does the order of invoking these two sets of callbacks matter?

    // Process the updated tables in the `subscription_update`.
    if let Some(update) = subscription_update {
        let mut callback_reminders = RowCallbackReminders::new_for_subscription_update(&update);
        let new_state = update_client_cache(client_cache, |client_cache| {
            process_subscription_update_for_transaction_update(update, client_cache, &mut callback_reminders);
        });

        let mut db_callbacks_lock = db_callbacks.lock().expect("DbCallbacks Mutex is poisoned");

        new_state.invoke_row_callbacks(&mut callback_reminders, &mut db_callbacks_lock);

        // Invoke reducer callbacks, if any, on the `event`.
        if let Some(event) = event {
            let mut reducer_lock = reducer_callbacks.lock().expect("ReducerCallbacks Mutex is poisoned");
            process_event(event, &mut reducer_lock, new_state);
        } else {
            log::error!("Received TransactionUpdate with no Event");
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
    client_cache: SharedCell<ClientCacheView>,
    db_callbacks: SharedCell<DbCallbacks>,
    reducer_callbacks: SharedCell<ReducerCallbacks>,
    credentials: SharedCell<CredentialStore>,
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

                let mut db_callbacks_lock = db_callbacks.lock().expect("DbCallbacks Mutex is poisoned");
                new_state.invoke_row_callbacks(&mut callback_reminders, &mut db_callbacks_lock);
            }
            client_api_messages::Message {
                r#type: Some(client_api_messages::message::Type::TransactionUpdate(transaction_update)),
            } => {
                log::info!("Message TransactionUpdate");

                process_transaction_update(transaction_update, &client_cache, &db_callbacks, &reducer_callbacks);
            }
            client_api_messages::Message {
                r#type: Some(client_api_messages::message::Type::IdentityToken(ident)),
            } => {
                log::info!("Message IdentityToken");
                let state = Arc::clone(&client_cache.lock().expect("ClientCache Mutex is poisoned"));
                let mut credentials_lock = credentials.lock().expect("Credentials Mutex is poisoned");
                credentials_lock.handle_identity_token(ident, state);
            }
            other => log::info!("Unknown message: {:?}", other),
        }
    }
}

impl BackgroundDbConnection {
    fn spawn_receiver(
        recv: mpsc::UnboundedReceiver<client_api_messages::Message>,
        runtime: &Runtime,
        client_cache: SharedCell<ClientCacheView>,
        db_callbacks: SharedCell<DbCallbacks>,
        reducer_callbacks: SharedCell<ReducerCallbacks>,
        credentials: SharedCell<CredentialStore>,
    ) -> JoinHandle<()> {
        runtime.spawn(receiver_loop(
            recv,
            client_cache,
            db_callbacks,
            reducer_callbacks,
            credentials,
        ))
    }
    /// Connect to a database named `db_name` accessible over the internet at the URI `host`.
    ///
    /// If `credentials` are supplied, they will be passed to the new connection to
    /// identify and authenticate the user. Otherwise, a set of `Credentials` will be
    /// generated by the server.
    ///
    /// `handle_table_update`, `handle_resubscribe` and `handle_function_call` are
    /// functions autogenerated by the SpaceTime CLI in `mod.rs` which dispatch on various
    /// messages from the server in order to deserialize incoming rows. The CLI will
    /// generate and export a function `connect` from the `mod.rs` which wraps this
    /// function and passes these arguments automatically.
    ///
    /// Users should not call `BackgroundDbConnection` directly; instead, call the
    /// `connect` function generated by the SpaceTime CLI.
    pub fn connect<Host>(
        host: Host,
        db_name: &str,
        credentials: Option<Credentials>,
        handle_table_update: crate::client_cache::HandleTableUpdateFn,
        handle_resubscribe: crate::client_cache::HandleTableUpdateFn,
        invoke_row_callbacks: crate::client_cache::InvokeCallbacksFn,
        handle_event: crate::callbacks::HandleEventFn,
    ) -> Result<Self>
    where
        Host: TryInto<http::Uri>,
        <Host as TryInto<http::Uri>>::Error: std::error::Error + Send + Sync + 'static,
    {
        let runtime = Arc::new(make_runtime()?);
        let connection = runtime.block_on(DbConnection::connect(host, db_name, credentials.as_ref()))?;
        let client_cache = Arc::new(Mutex::new(Arc::new(ClientCache::new(
            handle_table_update,
            handle_resubscribe,
            invoke_row_callbacks,
        ))));
        let db_callbacks = Arc::new(Mutex::new(DbCallbacks::new(runtime.clone())));
        let reducer_callbacks = Arc::new(Mutex::new(ReducerCallbacks::new(handle_event, runtime.clone())));
        let credentials = Arc::new(Mutex::new(CredentialStore::maybe_with_credentials(
            credentials,
            &runtime,
        )));
        let (websocket_loop_handle, recv_chan, send_chan) = connection.spawn_message_loop(&runtime);
        let recv_handle = Self::spawn_receiver(
            recv_chan,
            &runtime,
            client_cache.clone(),
            db_callbacks.clone(),
            reducer_callbacks.clone(),
            credentials.clone(),
        );
        Ok(BackgroundDbConnection {
            runtime,
            send_chan,
            websocket_loop_handle,
            recv_handle,
            client_cache,
            db_callbacks,
            reducer_callbacks,
            credentials,
        })
    }

    pub fn subscribe(&self, queries: &[&str]) {
        if let Err(e) = self.send_chan.unbounded_send(client_api_messages::Message {
            r#type: Some(client_api_messages::message::Type::Subscribe(
                client_api_messages::Subscribe {
                    query_strings: queries.iter().map(|&s| s.into()).collect(),
                },
            )),
        }) {
            // TODO: decide how to handle this error. Panic? Log? Return result? The only
            //       error here is that the channel is closed (it can't be full because
            //       it's unbounded), which means the sender loop has panicked. That
            //       suggests that on Err, we should join the sender's `JoinHandle` to get
            //       an error.
            panic!("Sender has closed: {:?}", e);
        };
    }

    pub fn invoke_reducer<R: Reducer>(&self, reducer: R) {
        if let Err(e) = self.send_chan.unbounded_send(client_api_messages::Message {
            r#type: Some(client_api_messages::message::Type::FunctionCall(
                client_api_messages::FunctionCall {
                    reducer: R::REDUCER_NAME.to_string(),
                    arg_bytes: bsatn::to_vec(&reducer).expect("Serializing reducer failed"),
                },
            )),
        }) {
            // TODO: decide how to handle this error. Panic? Log? Return result? The only
            //       error here is that the channel is closed (it can't be full because
            //       it's unbounded), which means the sender loop has panicked. That
            //       suggests that on Err, we should join the sender's `JoinHandle` to get
            //       an error.
            panic!("Sender has closed: {:?}", e);
        }
    }
}
