use crate::callbacks::{CredentialStore, DbCallbacks, ReducerCallbacks};
use crate::client_api_messages;
use crate::client_cache::{ClientCache, RowCallbackReminders};
use crate::identity::Credentials;
use crate::reducer::{AnyReducerEvent, Reducer};
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

pub struct BackgroundDbConnection {
    #[allow(unused)]
    runtime: Arc<Runtime>,
    send_chan: mpsc::UnboundedSender<client_api_messages::Message>,
    #[allow(unused)]
    websocket_loop_handle: JoinHandle<()>,
    #[allow(unused)]
    recv_handle: JoinHandle<()>,
    #[allow(unused)]
    pub(crate) credentials: Arc<Mutex<CredentialStore>>,
    pub(crate) client_cache: Arc<Mutex<Arc<ClientCache>>>,
    pub(crate) db_callbacks: Arc<Mutex<DbCallbacks>>,
    pub(crate) reducer_callbacks: Arc<Mutex<ReducerCallbacks>>,
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

fn process_event(
    msg: client_api_messages::Event,
    reducer_callbacks: &mut ReducerCallbacks,
    state: Arc<ClientCache>,
) -> Option<Arc<AnyReducerEvent>> {
    reducer_callbacks.handle_event(msg, state)
}

fn update_client_cache(
    client_cache: &Mutex<Arc<ClientCache>>,
    update: impl FnOnce(&mut ClientCache),
) -> Arc<ClientCache> {
    log::info!("Acquiring ClientCache Mutex");
    let mut cache_lock = client_cache.lock().expect("ClientCache Mutex is poisoned");
    log::info!("Got ClientCache Mutex");
    let mut new_state = ClientCache::clone(&cache_lock);
    update(&mut new_state);
    *cache_lock = Arc::new(new_state);
    Arc::clone(&cache_lock)
}

fn process_transaction_update(
    client_api_messages::TransactionUpdate {
        subscription_update,
        event,
    }: client_api_messages::TransactionUpdate,
    client_cache: &Mutex<Arc<ClientCache>>,
    db_callbacks: &Mutex<DbCallbacks>,
    reducer_callbacks: &Mutex<ReducerCallbacks>,
) {
    // Process the updated tables in the `subscription_update`.
    if let Some(update) = subscription_update {
        let mut callback_reminders = RowCallbackReminders::new_for_subscription_update(&update);
        let new_state = update_client_cache(client_cache, |client_cache| {
            process_subscription_update_for_transaction_update(update, client_cache, &mut callback_reminders);
        });

        log::info!("Acquiring DbCallbacks Mutex");
        let mut db_callbacks_lock = db_callbacks.lock().expect("DbCallbacks Mutex is poisoned");
        log::info!("Got DbCallbacks Mutex");

        // Invoke reducer callbacks, if any, on the `event`.
        if let Some(event) = event {
            log::info!("Acquiring ReducerCallbacks Mutex");
            let mut reducer_lock = reducer_callbacks.lock().expect("ReducerCallbacks Mutex is poisoned");
            log::info!("Got ReducerCallbacks Mutex");
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
    client_cache: Arc<Mutex<Arc<ClientCache>>>,
    db_callbacks: Arc<Mutex<DbCallbacks>>,
    reducer_callbacks: Arc<Mutex<ReducerCallbacks>>,
    credentials: Arc<Mutex<CredentialStore>>,
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

                log::info!("Acquiring DbCallbacks Mutex");
                let mut db_callbacks_lock = db_callbacks.lock().expect("DbCallbacks Mutex is poisoned");
                log::info!("Got DbCallbacks Mutex");
                new_state.invoke_row_callbacks(&mut callback_reminders, &mut db_callbacks_lock, None);
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
                log::info!("Acquiring ClientCache Mutex");
                let state = Arc::clone(&client_cache.lock().expect("ClientCache Mutex is poisoned"));
                log::info!("Got ClientCache Mutex");
                log::info!("Acquiring Credentials Mutex");
                let mut credentials_lock = credentials.lock().expect("Credentials Mutex is poisoned");
                log::info!("Got Credentials Mutex");
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
        client_cache: Arc<Mutex<Arc<ClientCache>>>,
        db_callbacks: Arc<Mutex<DbCallbacks>>,
        reducer_callbacks: Arc<Mutex<ReducerCallbacks>>,
        credentials: Arc<Mutex<CredentialStore>>,
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
