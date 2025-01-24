use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Instant;

use super::messages::{OneOffQueryResponseMessage, SerializableMessage};
use super::{message_handlers, ClientActorId, MessageHandleError};
use crate::error::DBError;
use crate::host::{ModuleHost, NoSuchModule, ReducerArgs, ReducerCallError, ReducerCallResult};
use crate::messages::websocket::Subscribe;
use crate::util::prometheus_handle::IntGaugeExt;
use crate::worker_metrics::WORKER_METRICS;
use derive_more::From;
use futures::prelude::*;
use spacetimedb_client_api_messages::websocket::{
    BsatnFormat, CallReducerFlags, Compression, FormatSwitch, JsonFormat, SubscribeSingle, Unsubscribe, WebsocketFormat,
};
use spacetimedb_lib::identity::RequestId;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::AbortHandle;

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum Protocol {
    Text,
    Binary,
}

impl Protocol {
    pub(crate) fn assert_matches_format_switch<B, J>(self, fs: &FormatSwitch<B, J>) {
        match (self, fs) {
            (Protocol::Text, FormatSwitch::Json(_)) | (Protocol::Binary, FormatSwitch::Bsatn(_)) => {}
            _ => unreachable!("requested protocol does not match output format"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ClientConfig {
    /// The client's desired protocol (format) when the host replies.
    pub protocol: Protocol,
    /// The client's desired (conditional) compression algorithm, if any.
    pub compression: Compression,
    /// Whether the client prefers full [`TransactionUpdate`]s
    /// rather than  [`TransactionUpdateLight`]s on a successful update.
    // TODO(centril): As more knobs are added, make this into a bitfield (when there's time).
    pub tx_update_full: bool,
}

impl ClientConfig {
    pub fn for_test() -> ClientConfig {
        Self {
            protocol: Protocol::Binary,
            compression: <_>::default(),
            tx_update_full: true,
        }
    }
}

#[derive(Debug)]
pub struct ClientConnectionSender {
    pub id: ClientActorId,
    pub config: ClientConfig,
    sendtx: mpsc::Sender<SerializableMessage>,
    abort_handle: AbortHandle,
    cancelled: AtomicBool,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientSendError {
    #[error("client disconnected")]
    Disconnected,
    #[error("client was not responding and has been disconnected")]
    Cancelled,
}

impl ClientConnectionSender {
    pub fn dummy_with_channel(id: ClientActorId, config: ClientConfig) -> (Self, mpsc::Receiver<SerializableMessage>) {
        let (sendtx, rx) = mpsc::channel(1);
        // just make something up, it doesn't need to be attached to a real task
        let abort_handle = match tokio::runtime::Handle::try_current() {
            Ok(h) => h.spawn(async {}).abort_handle(),
            Err(_) => tokio::runtime::Runtime::new().unwrap().spawn(async {}).abort_handle(),
        };
        (
            Self {
                id,
                config,
                sendtx,
                abort_handle,
                cancelled: AtomicBool::new(false),
            },
            rx,
        )
    }

    pub fn dummy(id: ClientActorId, config: ClientConfig) -> Self {
        Self::dummy_with_channel(id, config).0
    }

    pub fn send_message(&self, message: impl Into<SerializableMessage>) -> Result<(), ClientSendError> {
        self.send(message.into())
    }

    fn send(&self, message: SerializableMessage) -> Result<(), ClientSendError> {
        if self.cancelled.load(Relaxed) {
            return Err(ClientSendError::Cancelled);
        }
        self.sendtx.try_send(message).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => {
                // we've hit CLIENT_CHANNEL_CAPACITY messages backed up in
                // the channel, so forcibly kick the client
                tracing::warn!(identity = %self.id.identity, address = %self.id.address, "client channel capacity exceeded");
                self.abort_handle.abort();
                self.cancelled.store(true, Relaxed);
                ClientSendError::Cancelled
            }
            mpsc::error::TrySendError::Closed(_) => ClientSendError::Disconnected,
        })?;

        Ok(())
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct ClientConnection {
    sender: Arc<ClientConnectionSender>,
    pub replica_id: u64,
    pub module: ModuleHost,
    module_rx: watch::Receiver<ModuleHost>,
}

impl Deref for ClientConnection {
    type Target = ClientConnectionSender;
    fn deref(&self) -> &Self::Target {
        &self.sender
    }
}

#[derive(Debug, From)]
pub enum DataMessage {
    Text(String),
    Binary(Vec<u8>),
}

impl DataMessage {
    pub fn len(&self) -> usize {
        match self {
            DataMessage::Text(s) => s.len(),
            DataMessage::Binary(b) => b.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// if a client racks up this many messages in the queue without ACK'ing
// anything, we boot 'em.
const CLIENT_CHANNEL_CAPACITY: usize = 16 * KB;
const KB: usize = 1024;

impl ClientConnection {
    /// Returns an error if ModuleHost closed
    pub async fn spawn<Fut>(
        id: ClientActorId,
        config: ClientConfig,
        replica_id: u64,
        mut module_rx: watch::Receiver<ModuleHost>,
        actor: impl FnOnce(ClientConnection, mpsc::Receiver<SerializableMessage>) -> Fut,
    ) -> Result<ClientConnection, ReducerCallError>
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        // Add this client as a subscriber
        // TODO: Right now this is connecting clients directly to a replica, but their requests should be
        // logically subscribed to the database, not any particular replica. We should handle failover for
        // them and stuff. Not right now though.
        let module = module_rx.borrow_and_update().clone();
        module
            .call_identity_connected_disconnected(id.identity, id.address, true)
            .await?;

        let (sendtx, sendrx) = mpsc::channel::<SerializableMessage>(CLIENT_CHANNEL_CAPACITY);

        let db = module.info().database_identity;

        let (fut_tx, fut_rx) = oneshot::channel::<Fut>();
        // weird dance so that we can get an abort_handle into ClientConnection
        let abort_handle = tokio::spawn(async move {
            let Ok(fut) = fut_rx.await else { return };

            let _gauge_guard = WORKER_METRICS.connected_clients.with_label_values(&db).inc_scope();

            fut.await
        })
        .abort_handle();

        let sender = Arc::new(ClientConnectionSender {
            id,
            config,
            sendtx,
            abort_handle,
            cancelled: AtomicBool::new(false),
        });
        let this = Self {
            sender,
            replica_id,
            module,
            module_rx,
        };

        let actor_fut = actor(this.clone(), sendrx);
        // if this fails, the actor() function called .abort(), which like... okay, I guess?
        let _ = fut_tx.send(actor_fut);

        Ok(this)
    }

    pub fn dummy(
        id: ClientActorId,
        config: ClientConfig,
        replica_id: u64,
        mut module_rx: watch::Receiver<ModuleHost>,
    ) -> Self {
        let module = module_rx.borrow_and_update().clone();
        Self {
            sender: Arc::new(ClientConnectionSender::dummy(id, config)),
            replica_id,
            module,
            module_rx,
        }
    }

    pub fn sender(&self) -> Arc<ClientConnectionSender> {
        self.sender.clone()
    }

    #[inline]
    pub fn handle_message(
        &self,
        message: impl Into<DataMessage>,
        timer: Instant,
    ) -> impl Future<Output = Result<(), MessageHandleError>> + '_ {
        message_handlers::handle(self, message.into(), timer)
    }

    pub async fn watch_module_host(&mut self) -> Result<(), NoSuchModule> {
        match self.module_rx.changed().await {
            Ok(()) => {
                self.module = self.module_rx.borrow_and_update().clone();
                Ok(())
            }
            Err(_) => Err(NoSuchModule),
        }
    }

    pub async fn call_reducer(
        &self,
        reducer: &str,
        args: ReducerArgs,
        request_id: RequestId,
        timer: Instant,
        flags: CallReducerFlags,
    ) -> Result<ReducerCallResult, ReducerCallError> {
        let caller = match flags {
            CallReducerFlags::FullUpdate => Some(self.sender()),
            // Setting `sender = None` causes `eval_updates` to skip sending to the caller
            // as it has no access to the caller other than by id/addr.
            CallReducerFlags::NoSuccessNotify => None,
        };

        self.module
            .call_reducer(
                self.id.identity,
                Some(self.id.address),
                caller,
                Some(request_id),
                Some(timer),
                reducer,
                args,
            )
            .await
    }

    pub async fn subscribe_single(&self, subscription: SubscribeSingle, timer: Instant) -> Result<(), DBError> {
        let me = self.clone();
        tokio::task::spawn_blocking(move || {
            me.module
                .subscriptions()
                .add_subscription(me.sender, subscription, timer, None)
        })
        .await
        .unwrap() // TODO: is unwrapping right here?
    }

    pub async fn unsubscribe(&self, request: Unsubscribe, timer: Instant) -> Result<(), DBError> {
        let me = self.clone();
        tokio::task::spawn_blocking(move || me.module.subscriptions().remove_subscription(me.sender, request, timer))
            .await
            .unwrap() // TODO: is unwrapping right here?
    }

    pub async fn subscribe(&self, subscription: Subscribe, timer: Instant) -> Result<(), DBError> {
        let me = self.clone();
        tokio::task::spawn_blocking(move || {
            me.module
                .subscriptions()
                .add_legacy_subscriber(me.sender, subscription, timer, None)
        })
        .await
        .unwrap()
    }

    pub fn one_off_query_json(&self, query: &str, message_id: &[u8], timer: Instant) -> Result<(), anyhow::Error> {
        let response = self.one_off_query::<JsonFormat>(query, message_id, timer);
        self.send_message(response)?;
        Ok(())
    }

    pub fn one_off_query_bsatn(&self, query: &str, message_id: &[u8], timer: Instant) -> Result<(), anyhow::Error> {
        let response = self.one_off_query::<BsatnFormat>(query, message_id, timer);
        self.send_message(response)?;
        Ok(())
    }

    fn one_off_query<F: WebsocketFormat>(
        &self,
        query: &str,
        message_id: &[u8],
        timer: Instant,
    ) -> OneOffQueryResponseMessage<F> {
        let result = self.module.one_off_query::<F>(self.id.identity, query.to_owned());
        let message_id = message_id.to_owned();
        let total_host_execution_duration = timer.elapsed().into();
        match result {
            Ok(results) => OneOffQueryResponseMessage {
                message_id,
                error: None,
                results: vec![results],
                total_host_execution_duration,
            },
            Err(err) => OneOffQueryResponseMessage {
                message_id,
                error: Some(format!("{}", err)),
                results: vec![],
                total_host_execution_duration,
            },
        }
    }

    pub async fn disconnect(self) {
        self.module.disconnect_client(self.id).await
    }
}
