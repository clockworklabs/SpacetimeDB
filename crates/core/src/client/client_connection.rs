use std::collections::VecDeque;
use std::future::poll_fn;
use std::ops::Deref;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Instant, SystemTime};

use super::messages::{OneOffQueryResponseMessage, ProcedureResultMessage, SerializableMessage};
use super::{message_handlers, ClientActorId, MessageHandleError};
use crate::db::relational_db::RelationalDB;
use crate::error::DBError;
use crate::host::module_host::ClientConnectedError;
use crate::host::{FunctionArgs, ModuleHost, NoSuchModule, ReducerCallError, ReducerCallResult};
use crate::messages::websocket::Subscribe;
use crate::subscription::module_subscription_manager::BroadcastError;
use crate::util::asyncify;
use crate::util::prometheus_handle::IntGaugeExt;
use crate::worker_metrics::WORKER_METRICS;
use bytes::Bytes;
use bytestring::ByteString;
use derive_more::From;
use futures::prelude::*;
use prometheus::{Histogram, IntCounter, IntGauge};
use spacetimedb_auth::identity::{ConnectionAuthCtx, SpacetimeIdentityClaims};
use spacetimedb_client_api_messages::websocket::{
    BsatnFormat, CallReducerFlags, Compression, FormatSwitch, JsonFormat, SubscribeMulti, SubscribeSingle, Unsubscribe,
    UnsubscribeMulti,
};
use spacetimedb_durability::{DurableOffset, TxOffset};
use spacetimedb_lib::identity::RequestId;
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_lib::Identity;
use tokio::sync::mpsc::error::{SendError, TrySendError};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::AbortHandle;
use tracing::{trace, warn};

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
    /// If `true`, the client requests to receive updates for transactions
    /// confirmed to be durable. If `false`, updates will be delivered
    /// immediately.
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

/// A message to be sent to the client, along with the transaction offset it
/// was computed at, if available.
///
// TODO: Consider a different name, "ClientUpdate" is used elsewhere already.
#[derive(Debug)]
struct ClientUpdate {
    /// Transaction offset at which `message` was computed.
    ///
    /// This is only `Some` if `message` is a query result.
    ///
    /// If `Some` and [`ClientConfig::confirmed_reads`] is `true`,
    /// [`ClientConnectionReceiver`] will delay delivery until the durable
    /// offset of the database is equal to or greater than `tx_offset`.
    pub tx_offset: Option<TxOffset>,
    /// Type-erased outgoing message.
    pub message: SerializableMessage,
}

/// Types with access to the [`DurableOffset`] of a database.
///
/// Provided implementors are [`watch::Receiver<ModuleHost>`] and [`RelationalDB`].
///
/// The latter is mostly useful for tests, where no managed [`ModuleHost`] is
/// available, while the former supports module hotswapping.
pub trait DurableOffsetSupply: Send {
    /// Obtain the current [`DurableOffset`] handle.
    ///
    /// Returns:
    ///
    /// - `Err(NoSuchModule)` if the database was shut down
    /// - `Ok(None)` if the database is configured without durability
    /// - `Ok(Some(DurableOffset))` otherwise
    ///
    fn durable_offset(&mut self) -> Result<Option<DurableOffset>, NoSuchModule>;
}

impl DurableOffsetSupply for watch::Receiver<ModuleHost> {
    fn durable_offset(&mut self) -> Result<Option<DurableOffset>, NoSuchModule> {
        let module = if self.has_changed().map_err(|_| NoSuchModule)? {
            self.borrow_and_update()
        } else {
            self.borrow()
        };

        Ok(module.replica_ctx().relational_db.durable_tx_offset())
    }
}

impl DurableOffsetSupply for RelationalDB {
    fn durable_offset(&mut self) -> Result<Option<DurableOffset>, NoSuchModule> {
        Ok(self.durable_tx_offset())
    }
}

/// Receiving end of [`ClientConnectionSender`].
///
/// The [`ClientConnection`] actor reads messages from this channel and sends
/// them to the client over its websocket connection.
///
/// The [`ClientConnectionReceiver`] takes care of confirmed reads semantics,
/// if requested by the client.
pub struct ClientConnectionReceiver {
    confirmed_reads: bool,
    channel: MeteredReceiver<ClientUpdate>,
    current: Option<ClientUpdate>,
    offset_supply: Box<dyn DurableOffsetSupply>,
}

impl ClientConnectionReceiver {
    fn new(
        confirmed_reads: bool,
        channel: MeteredReceiver<ClientUpdate>,
        offset_supply: impl DurableOffsetSupply + 'static,
    ) -> Self {
        Self {
            confirmed_reads,
            channel,
            current: None,
            offset_supply: Box::new(offset_supply),
        }
    }

    /// Receive the next message from this channel.
    ///
    /// If this method returns `None`, the channel is closed and no more messages
    /// are in the internal buffers. No more messages can ever be received from
    /// the channel.
    ///
    /// Messages are returned immediately if:
    ///
    ///   - The (internal) [`ClientUpdate`] does not have a `tx_offset`
    ///     (such as for error messages).
    ///   - The client hasn't requested confirmed reads (i.e.
    ///     [`ClientConfig::confirmed_reads`] is `false`).
    ///   - The database is configured to not persist transactions.
    ///
    /// Otherwise, the update's `tx_offset` is compared against the module's
    /// durable offset. If the durable offset is behind the `tx_offset`, the
    /// method waits until it catches up before returning the message.
    ///
    /// If the database is shut down while waiting for the durable offset,
    /// `None` is returned. In this case, no more messages can ever be received
    /// from the channel.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe, as long as `self` is not dropped.
    ///
    /// If `recv` is used in a [`tokio::select!`] statement, it may get
    /// cancelled while waiting for the durable offset to catch up. At this
    /// point, it has already received a value from the underlying channel.
    /// This value is stored internally, so calling `recv` again will not lose
    /// data.
    //
    // TODO: Can we make a cancel-safe `recv_many` with confirmed reads semantics?
    pub async fn recv(&mut self) -> Option<SerializableMessage> {
        let ClientUpdate { tx_offset, message } = match self.current.take() {
            None => self.channel.recv().await?,
            Some(update) => update,
        };
        if !self.confirmed_reads {
            return Some(message);
        }

        if let Some(tx_offset) = tx_offset {
            match self.offset_supply.durable_offset() {
                Ok(Some(mut durable)) => {
                    // Store the current update in case we get cancelled while
                    // waiting for the durable offset.
                    self.current = Some(ClientUpdate {
                        tx_offset: Some(tx_offset),
                        message,
                    });
                    trace!("waiting for offset {tx_offset} to become durable");
                    durable
                        .wait_for(tx_offset)
                        .await
                        .inspect_err(|_| {
                            warn!("database went away while waiting for durable offset");
                        })
                        .ok()?;
                    self.current.take().map(|update| update.message)
                }
                // Database shut down or crashed.
                Err(NoSuchModule) => None,
                // In-memory database.
                Ok(None) => Some(message),
            }
        } else {
            Some(message)
        }
    }

    /// Close the receiver without dropping it.
    ///
    /// This is used to notify the [`ClientConnectionSender`] that the receiver
    /// will not consume any more messages from the channel, usually because the
    /// connection has been closed or is about to be closed.
    ///
    /// After calling this method, the sender will not be able to send more
    /// messages, preventing the internal buffer from filling up.
    pub fn close(&mut self) {
        self.channel.close();
    }
}

#[derive(Debug)]
pub struct ClientConnectionSender {
    pub id: ClientActorId,
    pub auth: ConnectionAuthCtx,
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
    pub fn dummy_with_channel(
        id: ClientActorId,
        config: ClientConfig,
        offset_supply: impl DurableOffsetSupply + 'static,
    ) -> (Self, ClientConnectionReceiver) {
        let (sendtx, rx) = mpsc::channel(CLIENT_CHANNEL_CAPACITY_TEST);
        // just make something up, it doesn't need to be attached to a real task
        let abort_handle = match tokio::runtime::Handle::try_current() {
            Ok(h) => h.spawn(async {}).abort_handle(),
            Err(_) => tokio::runtime::Runtime::new().unwrap().spawn(async {}).abort_handle(),
        };

        let receiver = ClientConnectionReceiver::new(config.confirmed_reads, MeteredReceiver::new(rx), offset_supply);
        let cancelled = AtomicBool::new(false);
        let dummy_claims = SpacetimeIdentityClaims {
            identity: id.identity,
            subject: "".to_string(),
            issuer: "".to_string(),
            audience: vec![],
            iat: SystemTime::now(),
            exp: None,
        };
        let sender = Self {
            id,
            auth: ConnectionAuthCtx::try_from(dummy_claims).expect("dummy claims should always be valid"),
            config,
            sendtx,
            abort_handle,
            cancelled,
            metrics: None,
        };
        (sender, receiver)
    }

    pub fn dummy(id: ClientActorId, config: ClientConfig, offset_supply: impl DurableOffsetSupply + 'static) -> Self {
        Self::dummy_with_channel(id, config, offset_supply).0
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Send a message to the client. For data-related messages, you should probably use
    /// `BroadcastQueue::send` to ensure that the client sees data messages in a consistent order.
    ///
    /// If `message` is the result of evaluating a query, then `tx_offset` should be
    /// the TX offset of the database state against which the query was evaluated.
    /// If `message` is not the result of evaluating a query (e.g. it reports an error),
    /// `tx_offset` should be `None`.
    /// For clients which have requested only confirmed durable reads,
    /// the sender will delay sending `message` until the `tx_offset` is confirmed.
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
                tracing::warn!(
                    identity = %self.id.identity,
                    connection_id = %self.id.connection_id,
                    confirmed_reads = self.config.confirmed_reads,
                    "client channel capacity exceeded"
                );
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

/// Wraps the transmitting end of a channel with a gauge for tracking the size of the channel.
pub struct MeteredSender<T> {
    inner: mpsc::Sender<T>,
    gauge: Option<IntGauge>,
}

impl<T> MeteredSender<T> {
    pub fn new(inner: mpsc::Sender<T>) -> Self {
        Self { inner, gauge: None }
    }

    pub fn with_gauge(inner: mpsc::Sender<T>, gauge: IntGauge) -> Self {
        Self {
            inner,
            gauge: Some(gauge),
        }
    }

    pub async fn send(&mut self, value: T) -> Result<(), SendError<T>> {
        self.inner.send(value).await?;
        if let Some(gauge) = &self.gauge {
            gauge.inc();
        }
        Ok(())
    }

    pub fn try_send(&mut self, value: T) -> Result<(), TrySendError<T>> {
        self.inner.try_send(value)?;
        if let Some(gauge) = &self.gauge {
            gauge.inc();
        }
        Ok(())
    }
}

// if a client racks up this many messages in the queue without ACK'ing
// anything, we boot 'em.
const CLIENT_CHANNEL_CAPACITY: usize = 16 * KB;
// use a smaller value for tests
const CLIENT_CHANNEL_CAPACITY_TEST: usize = 8;

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
        auth: ConnectionAuthCtx,
    ) -> Result<Connected, ClientConnectedError> {
        let module = module_rx.borrow_and_update().clone();
        module.call_identity_connected(auth, id.connection_id).await?;
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
        auth: ConnectionAuthCtx,
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
        let client_identity = id.identity;
        let abort_handle = tokio::spawn(async move {
            let Ok(fut) = fut_rx.await else { return };

            let _gauge_guard = module_info.metrics.connected_clients.inc_scope();
            module_info.metrics.ws_clients_spawned.inc();
            scopeguard::defer! {
                let database_identity = module_info.database_identity;
                log::warn!("websocket connection aborted for client identity `{client_identity}` and database identity `{database_identity}`");
                module_info.metrics.ws_clients_aborted.inc();
            };

            fut.await
        })
        .abort_handle();

        let metrics = ClientConnectionMetrics::new(database_identity, config.protocol);
        let receiver = ClientConnectionReceiver::new(
            config.confirmed_reads,
            MeteredReceiver::with_gauge(sendrx, metrics.sendtx_queue_size.clone()),
            module_rx.clone(),
        );

        let sender = Arc::new(ClientConnectionSender {
            id,
            auth,
            config,
            sendtx,
            abort_handle,
            cancelled: AtomicBool::new(false),
            metrics: Some(metrics),
        });
        let this = Self {
            sender,
            replica_id,
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
        module_rx: watch::Receiver<ModuleHost>,
    ) -> Self {
        Self {
            sender: Arc::new(ClientConnectionSender::dummy(id, config, module_rx.clone())),
            replica_id,
            module_rx,
        }
    }

    pub fn sender(&self) -> Arc<ClientConnectionSender> {
        self.sender.clone()
    }

    /// Get the [`ModuleHost`] for this connection.
    ///
    /// Note that modules can be hotswapped, in which case the returned handle
    /// becomes invalid (i.e. all calls on it will result in an error).
    /// Callers should thus drop the value as soon as they are done, and obtain
    /// a fresh one when needed.
    ///
    /// While this [`ClientConnection`] is active, [`Self::watch_module_host`]
    /// should be polled in the background, and the connection closed if and
    /// when it returns an error.
    pub fn module(&self) -> ModuleHost {
        self.module_rx.borrow().clone()
    }

    #[inline]
    pub fn handle_message(
        &self,
        message: impl Into<DataMessage>,
        timer: Instant,
    ) -> impl Future<Output = Result<(), MessageHandleError>> + '_ {
        message_handlers::handle(self, message.into(), timer)
    }

    /// Waits until the [`ModuleHost`] of this [`ClientConnection`] instance
    /// exits, in which case `Err` containing [`NoSuchModule`] is returned.
    ///
    /// Should be polled while this [`ClientConnection`] is active, so as to be
    /// able to shut down the connection gracefully if and when the module
    /// exits.
    ///
    /// Note that this borrows `self` mutably, so may require cloning the
    /// [`ClientConnection`] instance. The module is shared, however, so all
    /// clones will observe a swapped module.
    pub async fn watch_module_host(&mut self) -> Result<(), NoSuchModule> {
        loop {
            // First check if the module exited between creating the client
            // connection and calling `watch_module_host`...
            if self.module_rx.changed().await.is_err() {
                return Err(NoSuchModule);
            }
            // ...then mark the current module as seen, so the next iteration
            // of the loop waits until the module changes or exits.
            self.module_rx.mark_unchanged();
        }
    }

    pub async fn call_reducer(
        &self,
        reducer: &str,
        args: FunctionArgs,
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

        self.module()
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

    pub async fn call_procedure(
        &self,
        procedure: &str,
        args: FunctionArgs,
        request_id: RequestId,
        timer: Instant,
    ) -> Result<(), BroadcastError> {
        let res = self
            .module()
            .call_procedure(
                self.id.identity,
                Some(self.id.connection_id),
                Some(timer),
                procedure,
                args,
            )
            .await;

        self.module()
            .subscriptions()
            .send_procedure_message(self.sender(), ProcedureResultMessage::from_result(&res, request_id))
    }

    pub async fn subscribe_single(
        &self,
        subscription: SubscribeSingle,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
        let me = self.clone();
        self.module()
            .on_module_thread("subscribe_single", move || {
                me.module()
                    .subscriptions()
                    .add_single_subscription(me.sender, subscription, timer, None)
            })
            .await?
    }

    pub async fn unsubscribe(&self, request: Unsubscribe, timer: Instant) -> Result<Option<ExecutionMetrics>, DBError> {
        let me = self.clone();
        asyncify(move || {
            me.module()
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
        self.module()
            .on_module_thread("subscribe_multi", move || {
                me.module()
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
        self.module()
            .on_module_thread("unsubscribe_multi", move || {
                me.module()
                    .subscriptions()
                    .remove_multi_subscription(me.sender, request, timer)
            })
            .await?
    }

    pub async fn subscribe(&self, subscription: Subscribe, timer: Instant) -> Result<ExecutionMetrics, DBError> {
        let me = self.clone();
        asyncify(move || {
            me.module()
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
        self.module()
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
        self.module()
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
        self.module().disconnect_client(self.id).await
    }
}

#[cfg(test)]
mod tests {
    use core::fmt;
    use std::pin::pin;

    use pretty_assertions::assert_matches;

    use super::*;
    use crate::client::messages::{SubscriptionUpdateMessage, TransactionUpdateMessage};

    #[derive(Clone)]
    struct FakeDurableOffset {
        channel: watch::Sender<Option<TxOffset>>,
        closed: Arc<AtomicBool>,
    }

    impl DurableOffsetSupply for FakeDurableOffset {
        fn durable_offset(&mut self) -> Result<Option<DurableOffset>, NoSuchModule> {
            if self.closed.load(Ordering::Acquire) {
                Err(NoSuchModule)
            } else {
                Ok(Some(self.channel.subscribe().into()))
            }
        }
    }

    impl FakeDurableOffset {
        fn new() -> Self {
            let (tx, _) = watch::channel(None);
            Self {
                channel: tx,
                closed: <_>::default(),
            }
        }

        fn mark_durable_at(&self, offset: TxOffset) {
            self.channel.send_modify(|val| {
                val.replace(offset);
            })
        }

        fn close(&self) {
            self.closed.store(true, Ordering::Release);
        }
    }

    /// [DurableOffsetSupply] that only stores the receiver side of a watch
    /// channel initialized to some value.
    ///
    /// Calling `wait_for` will succeed while the provided value is smaller than
    /// or equal to the stored value, but report the channel as closed once it
    /// attempts to wait for a new value.
    struct DisconnectedDurableOffset {
        receiver: watch::Receiver<Option<TxOffset>>,
    }

    impl DisconnectedDurableOffset {
        fn new(offset: TxOffset) -> Self {
            let (_, rx) = watch::channel(Some(offset));
            Self { receiver: rx }
        }
    }

    impl DurableOffsetSupply for DisconnectedDurableOffset {
        fn durable_offset(&mut self) -> Result<Option<DurableOffset>, NoSuchModule> {
            Ok(Some(self.receiver.clone().into()))
        }
    }

    /// [DurableOffsetSupply] that always returns `Ok(None)`.
    struct NoneDurableOffset;

    impl DurableOffsetSupply for NoneDurableOffset {
        fn durable_offset(&mut self) -> Result<Option<DurableOffset>, NoSuchModule> {
            Ok(None)
        }
    }

    fn empty_tx_update() -> TransactionUpdateMessage {
        TransactionUpdateMessage {
            event: None,
            database_update: SubscriptionUpdateMessage::default_for_protocol(Protocol::Binary, None),
        }
    }

    async fn assert_received_update(f: impl Future<Output = Option<SerializableMessage>>) {
        assert_matches!(f.await, Some(SerializableMessage::TxUpdate(_)));
    }

    async fn assert_receiver_closed(f: impl Future<Output = Option<SerializableMessage>>) {
        assert_matches!(f.await, None);
    }

    async fn assert_pending(f: &mut (impl Future<Output: fmt::Debug> + Unpin)) {
        assert_matches!(futures::poll!(f), Poll::Pending);
    }

    fn default_client(
        offset_supply: impl DurableOffsetSupply + 'static,
    ) -> (ClientConnectionSender, ClientConnectionReceiver) {
        ClientConnectionSender::dummy_with_channel(
            ClientActorId::for_test(Identity::ZERO),
            ClientConfig {
                confirmed_reads: false,
                ..ClientConfig::for_test()
            },
            offset_supply,
        )
    }

    fn client_with_confirmed_reads(
        offset_supply: impl DurableOffsetSupply + 'static,
    ) -> (ClientConnectionSender, ClientConnectionReceiver) {
        ClientConnectionSender::dummy_with_channel(
            ClientActorId::for_test(Identity::ZERO),
            ClientConfig {
                confirmed_reads: true,
                ..ClientConfig::for_test()
            },
            offset_supply,
        )
    }

    #[tokio::test]
    async fn client_connection_receiver_waits_for_durable_offset() {
        let offset = FakeDurableOffset::new();
        let (sender, mut receiver) = client_with_confirmed_reads(offset.clone());

        for tx_offset in 0..10 {
            sender.send_message(Some(tx_offset), empty_tx_update()).unwrap();
            let mut recv = pin!(receiver.recv());
            assert_pending(&mut recv).await;
            offset.mark_durable_at(tx_offset);
            assert_received_update(recv).await;
        }
    }

    #[tokio::test]
    async fn client_connection_receiver_immediately_yields_message_if_already_durable() {
        let offset = FakeDurableOffset::new();
        let (sender, mut receiver) = client_with_confirmed_reads(offset.clone());

        for tx_offset in 0..10 {
            offset.mark_durable_at(tx_offset);
            sender.send_message(Some(tx_offset), empty_tx_update()).unwrap();
            assert_received_update(receiver.recv()).await;
        }
    }

    #[tokio::test]
    async fn client_connection_receiver_ends_if_durable_offset_closed() {
        let offset = FakeDurableOffset::new();
        let (sender, mut receiver) = client_with_confirmed_reads(offset.clone());

        offset.close();
        sender.send_message(Some(42), empty_tx_update()).unwrap();
        assert_receiver_closed(receiver.recv()).await;
    }

    #[tokio::test]
    async fn client_connection_receiver_ends_if_durable_offset_dropped() {
        const INITIAL_OFFSET: TxOffset = 1;
        let offset = DisconnectedDurableOffset::new(INITIAL_OFFSET);
        let (sender, mut receiver) = client_with_confirmed_reads(offset);

        for tx_offset in 0..=(INITIAL_OFFSET + 1) {
            sender.send_message(Some(tx_offset), empty_tx_update()).unwrap();
            if tx_offset <= INITIAL_OFFSET {
                assert_received_update(receiver.recv()).await;
            } else {
                assert_receiver_closed(receiver.recv()).await;
            }
        }
    }

    #[tokio::test]
    async fn client_connection_receiver_immediately_yields_message_if_sent_without_offset() {
        let offset = FakeDurableOffset::new();
        let (sender, mut receiver) = client_with_confirmed_reads(offset.clone());

        for _ in 0..10 {
            sender.send_message(None, empty_tx_update()).unwrap();
            assert_received_update(receiver.recv()).await;
        }

        offset.mark_durable_at(5);

        for _ in 0..10 {
            sender.send_message(None, empty_tx_update()).unwrap();
            assert_received_update(receiver.recv()).await;
        }
    }

    #[tokio::test]
    async fn client_connection_receiver_immediately_yields_message_for_client_without_confirmed_reads() {
        let offset = FakeDurableOffset::new();
        let (sender, mut receiver) = default_client(offset.clone());

        for tx_offset in 0..10 {
            sender.send_message(Some(tx_offset), empty_tx_update()).unwrap();
            assert_received_update(receiver.recv()).await;
        }

        offset.mark_durable_at(10);

        for tx_offset in 0..10 {
            sender.send_message(Some(tx_offset), empty_tx_update()).unwrap();
            assert_received_update(receiver.recv()).await;
        }
    }

    #[tokio::test]
    async fn client_connection_receiver_immediately_yields_message_without_durability() {
        let (sender, mut receiver) = client_with_confirmed_reads(NoneDurableOffset);

        for tx_offset in 0..10 {
            sender.send_message(Some(tx_offset), empty_tx_update()).unwrap();
            assert_received_update(receiver.recv()).await;
        }
    }

    #[tokio::test]
    async fn client_connection_receiver_cancel_safety() {
        let offset = FakeDurableOffset::new();
        let (sender, mut receiver) = client_with_confirmed_reads(offset.clone());

        sender.send_message(Some(3), empty_tx_update()).unwrap();
        assert_pending(&mut pin!(receiver.recv())).await;
        offset.mark_durable_at(3);
        assert_received_update(receiver.recv()).await;
    }
}
