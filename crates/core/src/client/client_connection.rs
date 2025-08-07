use std::collections::VecDeque;
use std::future::poll_fn;
use std::ops::Deref;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use super::messages::{OneOffQueryResponseMessage, SerializableMessage};
use super::{message_handlers, ClientActorId, MessageHandleError};
use crate::error::DBError;
use crate::host::module_host::ClientConnectedError;
use crate::host::{ModuleHost, NoSuchModule, ReducerArgs, ReducerCallError, ReducerCallResult};
use crate::messages::websocket::Subscribe;
use crate::util::asyncify;
use crate::util::prometheus_handle::IntGaugeExt;
use crate::worker_metrics::WORKER_METRICS;
use bytes::Bytes;
use bytestring::ByteString;
use derive_more::From;
use futures::prelude::*;
use prometheus::{Histogram, IntCounter, IntGauge};
use spacetimedb_client_api_messages::websocket::{
    BsatnFormat, CallReducerFlags, Compression, FormatSwitch, JsonFormat, SubscribeMulti, SubscribeSingle, Unsubscribe,
    UnsubscribeMulti,
};
use spacetimedb_durability::{DurableOffset, TxOffset};
use spacetimedb_lib::identity::RequestId;
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_lib::Identity;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::AbortHandle;

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum Protocol {
    Text,
    Binary,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::Text => "text",
            Protocol::Binary => "binary",
        }
    }

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
    /// Whether to send only confirmed (aka durable) transactions to the client.
    pub confirmed_reads: bool,
}

impl ClientConfig {
    pub fn for_test() -> ClientConfig {
        Self {
            protocol: Protocol::Binary,
            compression: <_>::default(),
            tx_update_full: true,
            confirmed_reads: false,
        }
    }
}

#[derive(Debug)]
pub struct ClientUpdate {
    pub tx_offset: Option<TxOffset>,
    pub message: SerializableMessage,
}

pub struct ClientConnectionReceiver {
    confirmed_reads: bool,
    channel: MeteredReceiver<ClientUpdate>,
    module: ModuleHost,
    module_rx: watch::Receiver<ModuleHost>,
}

impl ClientConnectionReceiver {
    pub async fn recv(&mut self) -> Option<SerializableMessage> {
        let ClientUpdate { tx_offset, message } = self.channel.recv().await?;
        if !self.confirmed_reads || tx_offset.is_none() {
            return Some(message);
        }

        if let Some(tx_offset) = tx_offset {
            if self.module_rx.has_changed().ok()? {
                let Some(mut durable) = self.durable_tx_offset().await else {
                    return Some(message);
                };
                let durable_offset = durable.get();
                if durable_offset.is_none() || durable_offset.is_some_and(|offset| offset < tx_offset) {
                    durable.wait_for(tx_offset).await.ok()?;
                }
            }
        }

        Some(message)
    }

    pub fn close(&mut self) {
        self.channel.close();
    }

    async fn durable_tx_offset(&mut self) -> Option<DurableOffset> {
        if self.module_rx.has_changed().ok()? {
            self.module = self.module_rx.borrow_and_update().clone();
        }

        self.module.replica_ctx().relational_db.durable_tx_offset()
    }
}

#[derive(Debug)]
pub struct ClientConnectionSender {
    pub id: ClientActorId,
    pub config: ClientConfig,
    sendtx: mpsc::Sender<ClientUpdate>,
    abort_handle: AbortHandle,
    cancelled: AtomicBool,

    /// Handles on Prometheus metrics related to connections to this database.
    ///
    /// Will be `None` when constructed by [`ClientConnectionSender::dummy_with_channel`]
    /// or [`ClientConnectionSender::dummy`], which are used in tests.
    /// Will be `Some` whenever this `ClientConnectionSender` is wired up to an actual client connection.
    metrics: Option<ClientConnectionMetrics>,
}

#[derive(Debug)]
pub struct ClientConnectionMetrics {
    pub websocket_request_msg_size: Histogram,
    pub websocket_requests: IntCounter,

    /// The `total_outgoing_queue_length` metric labeled with this database's `Identity`,
    /// which we'll increment whenever sending a message.
    ///
    /// This metric will be decremented, and cleaned up,
    /// by `ws_client_actor_inner` in client-api/src/routes/subscribe.rs.
    /// Care must be taken not to increment it after the client has disconnected
    /// and performed its clean-up.
    pub sendtx_queue_size: IntGauge,
}

impl ClientConnectionMetrics {
    fn new(database_identity: Identity, protocol: Protocol) -> Self {
        let message_kind = protocol.as_str();
        let websocket_request_msg_size = WORKER_METRICS
            .websocket_request_msg_size
            .with_label_values(&database_identity, message_kind);
        let websocket_requests = WORKER_METRICS
            .websocket_requests
            .with_label_values(&database_identity, message_kind);
        let sendtx_queue_size = WORKER_METRICS
            .total_outgoing_queue_length
            .with_label_values(&database_identity);

        Self {
            websocket_request_msg_size,
            websocket_requests,
            sendtx_queue_size,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClientSendError {
    #[error("client disconnected")]
    Disconnected,
    #[error("client was not responding and has been disconnected")]
    Cancelled,
}

impl ClientConnectionSender {
    pub fn dummy_with_channel(id: ClientActorId, config: ClientConfig) -> (Self, MeteredReceiver<ClientUpdate>) {
        let (sendtx, rx) = mpsc::channel(1);
        // just make something up, it doesn't need to be attached to a real task
        let abort_handle = match tokio::runtime::Handle::try_current() {
            Ok(h) => h.spawn(async {}).abort_handle(),
            Err(_) => tokio::runtime::Runtime::new().unwrap().spawn(async {}).abort_handle(),
        };

        let rx = MeteredReceiver::new(rx);
        let cancelled = AtomicBool::new(false);
        let sender = Self {
            id,
            config,
            sendtx,
            abort_handle,
            cancelled,
            metrics: None,
        };
        (sender, rx)
    }

    pub fn dummy(id: ClientActorId, config: ClientConfig) -> Self {
        Self::dummy_with_channel(id, config).0
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Send a message to the client. For data-related messages, you should probably use
    /// `BroadcastQueue::send` to ensure that the client sees data messages in a consistent order.
    pub fn send_message(
        &self,
        tx_offset: Option<TxOffset>,
        message: impl Into<SerializableMessage>,
    ) -> Result<(), ClientSendError> {
        self.send(ClientUpdate {
            tx_offset,
            message: message.into(),
        })
    }

    fn send(&self, message: ClientUpdate) -> Result<(), ClientSendError> {
        if self.cancelled.load(Relaxed) {
            return Err(ClientSendError::Cancelled);
        }

        match self.sendtx.try_send(message) {
            Err(mpsc::error::TrySendError::Full(_)) => {
                // we've hit CLIENT_CHANNEL_CAPACITY messages backed up in
                // the channel, so forcibly kick the client
                tracing::warn!(identity = %self.id.identity, connection_id = %self.id.connection_id, "client channel capacity exceeded");
                self.abort_handle.abort();
                self.cancelled.store(true, Ordering::Relaxed);
                return Err(ClientSendError::Cancelled);
            }
            Err(mpsc::error::TrySendError::Closed(_)) => return Err(ClientSendError::Disconnected),
            Ok(()) => {
                // If we successfully pushed a message into the queue, increment the queue size metric.
                // Don't do this before pushing because, if the client has disconnected,
                // it will already have performed its clean-up,
                // and so would never perform the corresponding `dec` to this `inc`.
                if let Some(metrics) = &self.metrics {
                    metrics.sendtx_queue_size.inc();
                }
            }
        }

        Ok(())
    }

    pub(crate) fn observe_websocket_request_message(&self, message: &DataMessage) {
        if let Some(metrics) = &self.metrics {
            metrics.websocket_request_msg_size.observe(message.len() as f64);
            metrics.websocket_requests.inc();
        }
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
    Text(ByteString),
    Binary(Bytes),
}

impl From<String> for DataMessage {
    fn from(value: String) -> Self {
        ByteString::from(value).into()
    }
}

impl From<Vec<u8>> for DataMessage {
    fn from(value: Vec<u8>) -> Self {
        Bytes::from(value).into()
    }
}

impl DataMessage {
    /// Returns the number of bytes this message consists of.
    pub fn len(&self) -> usize {
        match self {
            Self::Text(s) => s.len(),
            Self::Binary(b) => b.len(),
        }
    }

    /// Is the message empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a handle to the underlying allocation of the message without consuming it.
    pub fn allocation(&self) -> Bytes {
        match self {
            DataMessage::Text(alloc) => alloc.as_bytes().clone(),
            DataMessage::Binary(alloc) => alloc.clone(),
        }
    }
}

/// Wraps a [VecDeque] with a gauge for tracking its size.
/// We subtract its size from the gauge on drop to avoid leaking the metric.
pub struct MeteredDeque<T> {
    inner: VecDeque<T>,
    gauge: IntGauge,
}

impl<T> MeteredDeque<T> {
    pub fn new(gauge: IntGauge) -> Self {
        Self {
            inner: VecDeque::new(),
            gauge,
        }
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.inner.pop_front().inspect(|_| {
            self.gauge.dec();
        })
    }

    pub fn pop_back(&mut self) -> Option<T> {
        self.inner.pop_back().inspect(|_| {
            self.gauge.dec();
        })
    }

    pub fn push_front(&mut self, value: T) {
        self.gauge.inc();
        self.inner.push_front(value);
    }

    pub fn push_back(&mut self, value: T) {
        self.gauge.inc();
        self.inner.push_back(value);
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl<T> Drop for MeteredDeque<T> {
    fn drop(&mut self) {
        // Record the number of elements still in the deque on drop
        self.gauge.sub(self.inner.len() as _);
    }
}

/// Wraps the receiving end of a channel with a gauge for tracking the size of the channel.
/// We subtract the size of the channel from the gauge on drop to avoid leaking the metric.
pub struct MeteredReceiver<T> {
    inner: mpsc::Receiver<T>,
    gauge: Option<IntGauge>,
}

impl<T> MeteredReceiver<T> {
    pub fn new(inner: mpsc::Receiver<T>) -> Self {
        Self { inner, gauge: None }
    }

    pub fn with_gauge(inner: mpsc::Receiver<T>, gauge: IntGauge) -> Self {
        Self {
            inner,
            gauge: Some(gauge),
        }
    }

    pub async fn recv(&mut self) -> Option<T> {
        poll_fn(|cx| self.poll_recv(cx)).await
    }

    pub async fn recv_many(&mut self, buf: &mut Vec<T>, max: usize) -> usize {
        poll_fn(|cx| self.poll_recv_many(cx, buf, max)).await
    }

    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.inner.poll_recv(cx).map(|maybe_item| {
            maybe_item.inspect(|_| {
                if let Some(gauge) = &self.gauge {
                    gauge.dec()
                }
            })
        })
    }

    pub fn poll_recv_many(&mut self, cx: &mut Context<'_>, buf: &mut Vec<T>, max: usize) -> Poll<usize> {
        self.inner.poll_recv_many(cx, buf, max).map(|n| {
            if let Some(gauge) = &self.gauge {
                gauge.sub(n as _);
            }
            n
        })
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn close(&mut self) {
        self.inner.close();
    }
}

impl<T> Drop for MeteredReceiver<T> {
    fn drop(&mut self) {
        // Record the number of elements still in the channel on drop
        if let Some(gauge) = &self.gauge {
            gauge.sub(self.inner.len() as _);
        }
    }
}

// if a client racks up this many messages in the queue without ACK'ing
// anything, we boot 'em.
const CLIENT_CHANNEL_CAPACITY: usize = 16 * KB;
const KB: usize = 1024;

/// Value returned by [`ClientConnection::call_client_connected_maybe_reject`]
/// and consumed by [`ClientConnection::spawn`] which acts as a proof that the client is authorized.
///
/// Because this struct does not capture the module or database info or the client connection info,
/// a malicious caller could [`ClientConnected::call_client_connected_maybe_reject`] for one client
/// and then use the resulting `Connected` token to [`ClientConnection::spawn`] for a different client.
/// We're not particularly worried about that.
/// This token exists as a sanity check that non-malicious callers don't accidentally [`ClientConnection::spawn`]
/// for an unauthorized client.
#[non_exhaustive]
pub struct Connected {
    _private: (),
}

impl ClientConnection {
    /// Call the database at `module_rx`'s `client_connection` reducer, if any,
    /// and return `Err` if it signals rejecting this client's connection.
    ///
    /// Call this method before [`Self::spawn`]
    /// and pass the returned [`Connected`] to [`Self::spawn`] as proof that the client is authorized.
    pub async fn call_client_connected_maybe_reject(
        module_rx: &mut watch::Receiver<ModuleHost>,
        id: ClientActorId,
    ) -> Result<Connected, ClientConnectedError> {
        let module = module_rx.borrow_and_update().clone();
        module.call_identity_connected(id.identity, id.connection_id).await?;
        Ok(Connected { _private: () })
    }

    /// Spawn a new [`ClientConnection`] for a WebSocket subscriber.
    ///
    /// Callers should first call [`Self::call_client_connected_maybe_reject`]
    /// to verify that the database at `module_rx` approves of this connection,
    /// and should not invoke this method if that call returns an error,
    /// and pass the returned [`Connected`] as `_proof_of_client_connected_call`.
    pub async fn spawn<Fut>(
        id: ClientActorId,
        config: ClientConfig,
        replica_id: u64,
        mut module_rx: watch::Receiver<ModuleHost>,
        actor: impl FnOnce(ClientConnection, ClientConnectionReceiver) -> Fut,
        _proof_of_client_connected_call: Connected,
    ) -> ClientConnection
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        // Add this client as a subscriber
        // TODO: Right now this is connecting clients directly to a replica, but their requests should be
        // logically subscribed to the database, not any particular replica. We should handle failover for
        // them and stuff. Not right now though.
        let module = module_rx.borrow_and_update().clone();

        let (sendtx, sendrx) = mpsc::channel::<ClientUpdate>(CLIENT_CHANNEL_CAPACITY);

        let (fut_tx, fut_rx) = oneshot::channel::<Fut>();
        // weird dance so that we can get an abort_handle into ClientConnection
        let module_info = module.info.clone();
        let database_identity = module_info.database_identity;
        let abort_handle = tokio::spawn(async move {
            let Ok(fut) = fut_rx.await else { return };

            let _gauge_guard = module_info.metrics.connected_clients.inc_scope();
            module_info.metrics.ws_clients_spawned.inc();
            scopeguard::defer! {
                let database_identity = module_info.database_identity;
                let client_identity = id.identity;
                log::warn!("websocket connection aborted for client identity `{client_identity}` and database identity `{database_identity}`");
                module_info.metrics.ws_clients_aborted.inc();
            };

            fut.await
        })
        .abort_handle();

        let metrics = ClientConnectionMetrics::new(database_identity, config.protocol);
        let receiver = ClientConnectionReceiver {
            confirmed_reads: config.confirmed_reads,
            channel: MeteredReceiver::with_gauge(sendrx, metrics.sendtx_queue_size.clone()),
            module: module.clone(),
            module_rx: module_rx.clone(),
        };

        let sender = Arc::new(ClientConnectionSender {
            id,
            config,
            sendtx,
            abort_handle,
            cancelled: AtomicBool::new(false),
            metrics: Some(metrics),
        });
        let this = Self {
            sender,
            replica_id,
            module,
            module_rx,
        };

        let actor_fut = actor(this.clone(), receiver);
        // if this fails, the actor() function called .abort(), which like... okay, I guess?
        let _ = fut_tx.send(actor_fut);

        this
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

    pub fn durable_tx_offset(&self) -> Option<TxOffset> {
        self.module
            .replica_ctx()
            .relational_db
            .durable_tx_offset()
            .and_then(|durable_offset| durable_offset.get())
    }

    pub async fn wait_durable(&self, offset: TxOffset) -> Result<TxOffset, ()> {
        let Some(mut durable_offset) = self.module.replica_ctx().relational_db.durable_tx_offset() else {
            return Ok(offset);
        };
        durable_offset.wait_for(offset).await
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
            // as it has no access to the caller other than by id/connection id.
            CallReducerFlags::NoSuccessNotify => None,
        };

        self.module
            .call_reducer(
                self.id.identity,
                Some(self.id.connection_id),
                caller,
                Some(request_id),
                Some(timer),
                reducer,
                args,
            )
            .await
    }

    pub async fn subscribe_single(
        &self,
        subscription: SubscribeSingle,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
        let me = self.clone();
        self.module
            .on_module_thread("subscribe_single", move || {
                me.module
                    .subscriptions()
                    .add_single_subscription(me.sender, subscription, timer, None)
            })
            .await?
    }

    pub async fn unsubscribe(&self, request: Unsubscribe, timer: Instant) -> Result<Option<ExecutionMetrics>, DBError> {
        let me = self.clone();
        asyncify(move || {
            me.module
                .subscriptions()
                .remove_single_subscription(me.sender, request, timer)
        })
        .await
    }

    pub async fn subscribe_multi(
        &self,
        request: SubscribeMulti,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
        let me = self.clone();
        self.module
            .on_module_thread("subscribe_multi", move || {
                me.module
                    .subscriptions()
                    .add_multi_subscription(me.sender, request, timer, None)
            })
            .await?
    }

    pub async fn unsubscribe_multi(
        &self,
        request: UnsubscribeMulti,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
        let me = self.clone();
        self.module
            .on_module_thread("unsubscribe_multi", move || {
                me.module
                    .subscriptions()
                    .remove_multi_subscription(me.sender, request, timer)
            })
            .await?
    }

    pub async fn subscribe(&self, subscription: Subscribe, timer: Instant) -> Result<ExecutionMetrics, DBError> {
        let me = self.clone();
        asyncify(move || {
            me.module
                .subscriptions()
                .add_legacy_subscriber(me.sender, subscription, timer, None)
        })
        .await
    }

    pub async fn one_off_query_json(
        &self,
        query: &str,
        message_id: &[u8],
        timer: Instant,
    ) -> Result<(), anyhow::Error> {
        self.module
            .one_off_query::<JsonFormat>(
                self.id.identity,
                query.to_owned(),
                self.sender.clone(),
                message_id.to_owned(),
                timer,
                |msg: OneOffQueryResponseMessage<JsonFormat>| msg.into(),
            )
            .await
    }

    pub async fn one_off_query_bsatn(
        &self,
        query: &str,
        message_id: &[u8],
        timer: Instant,
    ) -> Result<(), anyhow::Error> {
        self.module
            .one_off_query::<BsatnFormat>(
                self.id.identity,
                query.to_owned(),
                self.sender.clone(),
                message_id.to_owned(),
                timer,
                |msg: OneOffQueryResponseMessage<BsatnFormat>| msg.into(),
            )
            .await
    }

    pub async fn disconnect(self) {
        self.module.disconnect_client(self.id).await
    }
}
