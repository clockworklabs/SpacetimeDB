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
use serde::Deserialize;
use spacetimedb_lib::identity::RequestId;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::AbortHandle;

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum Protocol {
    Text,
    Binary,
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug, Default, Deserialize)]
pub enum ProtocolCodec {
    None,
    Gzip,
    #[default]
    Brotli,
}

#[derive(Debug)]
pub struct ClientConnectionSender {
    pub id: ClientActorId,
    pub protocol: Protocol,
    pub codec: ProtocolCodec,
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
    pub fn dummy_with_channel(
        id: ClientActorId,
        protocol: Protocol,
        codec: ProtocolCodec,
    ) -> (Self, mpsc::Receiver<SerializableMessage>) {
        let (sendtx, rx) = mpsc::channel(1);
        // just make something up, it doesn't need to be attached to a real task
        let abort_handle = match tokio::runtime::Handle::try_current() {
            Ok(h) => h.spawn(async {}).abort_handle(),
            Err(_) => tokio::runtime::Runtime::new().unwrap().spawn(async {}).abort_handle(),
        };
        (
            Self {
                id,
                protocol,
                codec,
                sendtx,
                abort_handle,
                cancelled: AtomicBool::new(false),
            },
            rx,
        )
    }

    pub fn dummy(id: ClientActorId, protocol: Protocol, codec: ProtocolCodec) -> Self {
        Self::dummy_with_channel(id, protocol, codec).0
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
    pub database_instance_id: u64,
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
    pub async fn spawn<F, Fut>(
        id: ClientActorId,
        protocol: Protocol,
        codec: ProtocolCodec,
        database_instance_id: u64,
        mut module_rx: watch::Receiver<ModuleHost>,
        actor: F,
    ) -> Result<ClientConnection, ReducerCallError>
    where
        F: FnOnce(ClientConnection, mpsc::Receiver<SerializableMessage>) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        // Add this client as a subscriber
        // TODO: Right now this is connecting clients directly to an instance, but their requests should be
        // logically subscribed to the database, not any particular instance. We should handle failover for
        // them and stuff. Not right now though.
        let module = module_rx.borrow_and_update().clone();
        module
            .call_identity_connected_disconnected(id.identity, id.address, true)
            .await?;

        let (sendtx, sendrx) = mpsc::channel::<SerializableMessage>(CLIENT_CHANNEL_CAPACITY);

        let db = module.info().address;

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
            protocol,
            codec,
            sendtx,
            abort_handle,
            cancelled: AtomicBool::new(false),
        });
        let this = Self {
            sender,
            database_instance_id,
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
        protocol: Protocol,
        codec: ProtocolCodec,
        database_instance_id: u64,
        mut module_rx: watch::Receiver<ModuleHost>,
    ) -> Self {
        let module = module_rx.borrow_and_update().clone();
        Self {
            sender: Arc::new(ClientConnectionSender::dummy(id, protocol, codec)),
            database_instance_id,
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
    ) -> Result<ReducerCallResult, ReducerCallError> {
        self.module
            .call_reducer(
                self.id.identity,
                Some(self.id.address),
                Some(self.sender()),
                Some(request_id),
                Some(timer),
                reducer,
                args,
            )
            .await
    }

    pub async fn subscribe(&self, subscription: Subscribe, timer: Instant) -> Result<(), DBError> {
        let me = self.clone();
        tokio::task::spawn_blocking(move || {
            me.module
                .subscriptions()
                .add_subscriber(me.sender, subscription, timer, None)
        })
        .await
        .unwrap()
    }

    pub fn one_off_query(&self, query: &str, message_id: &[u8], timer: Instant) -> Result<(), anyhow::Error> {
        let result = self.module.one_off_query(self.id.identity, query.to_owned());
        let message_id = message_id.to_owned();
        let total_host_execution_duration = timer.elapsed().as_micros() as u64;
        let response = match result {
            Ok(results) => OneOffQueryResponseMessage {
                message_id,
                error: None,
                results,
                total_host_execution_duration,
            },
            Err(err) => OneOffQueryResponseMessage {
                message_id,
                error: Some(format!("{}", err)),
                results: Vec::new(),
                total_host_execution_duration,
            },
        };
        self.send_message(response)?;
        Ok(())
    }

    pub async fn disconnect(self) {
        self.module.disconnect_client(self.id).await
    }
}
