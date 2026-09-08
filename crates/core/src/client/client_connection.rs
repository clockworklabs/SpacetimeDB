use std::collections::VecDeque;
use std::future::poll_fn;
use std::ops::Deref;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Instant, SystemTime};

use super::{message_handlers, ClientActorId, MessageHandleError, OutboundMessage};
use crate::auth::{hosted_tokens::VerifiedHostedAuth, invocation::check_hosted_admission};
use crate::db::relational_db::RelationalDB;
use crate::error::DBError;
use crate::host::module_host::{ClientConnectedError, ProcedureResultTarget};
use crate::host::{FunctionArgs, ModuleHost, NoSuchModule, ReducerCallError};
use crate::subscription::module_subscription_manager::BroadcastError;
use crate::util::prometheus_handle::IntGaugeExt;
use crate::worker_metrics::WORKER_METRICS;
use bytes::Bytes;
use bytestring::ByteString;
use derive_more::From;
use futures::prelude::*;
use log::warn;
use prometheus::{Histogram, IntCounter, IntGauge};
use spacetimedb_auth::identity::{ConnectionAuthCtx, SpacetimeIdentityClaims};
use spacetimedb_client_api_messages::websocket::{common as ws_common, v1 as ws_v1, v2 as ws_v2};
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_durability::{DurableOffset, TxOffset};
use spacetimedb_lib::identity::{AuthCtx, RequestId};
use spacetimedb_lib::metrics::ExecutionMetrics;
use spacetimedb_lib::Identity;
use tokio::sync::mpsc::error::{SendError, TrySendError};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::AbortHandle;
use tracing::trace;

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum Protocol {
    Text,
    Binary,
}

#[derive(PartialEq, Eq, Clone, Copy, Hash, Debug)]
pub enum WsVersion {
    V1,
    V2,
    V3,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::Text => "text",
            Protocol::Binary => "binary",
        }
    }

    pub(crate) fn assert_matches_format_switch<B, J>(self, fs: &ws_v1::FormatSwitch<B, J>) {
        match (self, fs) {
            (Protocol::Text, ws_v1::FormatSwitch::Json(_)) | (Protocol::Binary, ws_v1::FormatSwitch::Bsatn(_)) => {}
            _ => unreachable!("requested protocol does not match output format"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ClientConfig {
    /// The client's desired protocol (format) when the host replies.
    pub protocol: Protocol,
    /// The websocket protocol version negotiated during the handshake.
    pub version: WsVersion,
    /// The client's desired (conditional) compression algorithm, if any.
    pub compression: ws_common::Compression,
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
            version: WsVersion::V1,
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
    pub message: OutboundMessage,
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

    /// Recheck the authoritative target state, never a cached generation.
    fn check_hosted_auth(
        &mut self,
        _proof: &VerifiedHostedAuth,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<()>> {
        Box::pin(async { anyhow::bail!("hosted connection has no authoritative database state") })
    }
}

impl DurableOffsetSupply for watch::Receiver<ModuleHost> {
    fn check_hosted_auth(
        &mut self,
        proof: &VerifiedHostedAuth,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<()>> {
        if self.has_changed().is_err() {
            return Box::pin(async { Err(NoSuchModule.into()) });
        }
        let module = self.borrow().clone();
        let mut db = module.relational_db().clone();
        db.check_hosted_auth(proof)
    }

    fn durable_offset(&mut self) -> Result<Option<DurableOffset>, NoSuchModule> {
        let module = if self.has_changed().map_err(|_| NoSuchModule)? {
            self.borrow_and_update()
        } else {
            self.borrow()
        };

        Ok(module.relational_db().durable_tx_offset())
    }
}

impl DurableOffsetSupply for Arc<RelationalDB> {
    fn check_hosted_auth(
        &mut self,
        proof: &VerifiedHostedAuth,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<()>> {
        let db = self.clone();
        let proof = proof.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                db.with_read_only(Workload::Internal, |tx| check_hosted_admission(tx, &db, Some(&proof)))
            })
            .await?
        })
    }

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
    pending: Vec<ClientUpdate>,
    offset_supply: Box<dyn DurableOffsetSupply>,
    hosted_sender: Option<std::sync::Weak<ClientConnectionSender>>,
}

impl ClientConnectionReceiver {
    pub const DEFAULT_RECV_MANY_LIMIT: usize = 4096;

    fn new(
        confirmed_reads: bool,
        channel: MeteredReceiver<ClientUpdate>,
        offset_supply: impl DurableOffsetSupply + 'static,
    ) -> Self {
        Self {
            confirmed_reads,
            channel,
            pending: Vec::new(),
            offset_supply: Box::new(offset_supply),
            hosted_sender: None,
        }
    }

    #[cfg(test)]
    pub(crate) async fn recv(&mut self) -> Option<OutboundMessage> {
        let mut buf = Vec::with_capacity(1);
        (self.recv_many(&mut buf, 1).await != 0).then(|| buf.remove(0))
    }

    /// Receive multiple messages from this channel.
    ///
    /// Messages are returned immediately if:
    ///
    ///   - The [`ClientUpdate`] does not have a `tx_offset`
    ///     (such as for error messages).
    ///   - The client hasn't requested confirmed reads
    ///     (i.e. [`ClientConfig::confirmed_reads`] is `false`).
    ///   - The database is configured to not persist transactions.
    ///
    /// Otherwise, the last `tx_offset` in the batch is compared against the module's
    /// durable offset. If the durable offset is behind the `tx_offset`, the
    /// method waits until it catches up before returning the message.
    ///
    /// If the database is shut down while waiting for the durable offset,
    /// 0 is returned. In this case, no more messages can ever be received
    /// from the channel.
    ///
    /// For non-zero values of `max`, this method will never return `0` unless the
    /// input channel has been closed and there are no pending messages, or if the
    /// database goes away. This indicates that no further values can ever be received
    /// from this `Receiver`.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe, as long as `self` is not dropped.
    ///
    /// If `recv_many` is used in a [`tokio::select!`] statement, it may get
    /// cancelled while waiting for the durable offset to catch up. At this
    /// point, it has already received values from the underlying channel.
    /// These values are stored internally, so calling `recv_many` again will
    /// not lose data.
    pub async fn recv_many(&mut self, buf: &mut Vec<OutboundMessage>, max: usize) -> usize {
        if !self.hosted_connection_is_valid().await {
            return 0;
        }
        // If there are no pending updates and the input channel has been closed,
        // no more messages can be received from this receiver.
        if max == 0 || (self.pending.is_empty() && self.channel.recv_many(&mut self.pending, max).await == 0) {
            return 0;
        }

        // If we don't have to wait for txns to be made durable,
        // drain the pending updates.
        if !self.confirmed_reads {
            return self.drain_pending(buf, max).await;
        }

        // If we do have to wait for txns to be made durable,
        // but the next client update doesn't have a tx offset,
        // there's no reason to wait - just send it.
        if !self.pending_update_has_offset() {
            return self.drain_pending(buf, 1).await;
        }

        // Otherwise, grab the next offset that we should wait for.
        let (n, wait_for_offset) = self.next_confirmed_reads_batch(max);

        match self.offset_supply.durable_offset() {
            Ok(Some(mut durable)) => {
                trace!("waiting for offset {wait_for_offset} to become durable");
                if durable.wait_for(wait_for_offset).await.is_err() {
                    warn!("database went away while waiting for durable offset");
                    return 0;
                }
                self.drain_pending(buf, n).await
            }
            // Database shut down or crashed.
            Err(NoSuchModule) => 0,
            // In-memory database.
            Ok(None) => self.drain_pending(buf, max).await,
        }
    }

    /// Compute the next batch of pending client updates that have a tx offset.
    /// What is the size of the batch and what is the max offset?
    fn next_confirmed_reads_batch(&self, max: usize) -> (usize, TxOffset) {
        self.pending
            .iter()
            .take(max)
            .map_while(|update| update.tx_offset)
            .fold((0, 0), |(count, max_offset), tx_offset| {
                (count + 1, max_offset.max(tx_offset))
            })
    }

    /// Drain the pending [`ClientUpdate`]s, up to `max, into `buf`.
    async fn drain_pending(&mut self, buf: &mut Vec<OutboundMessage>, max: usize) -> usize {
        // A queued update may predate revocation, and a confirmed-read wait may
        // outlast the credential. Check again immediately before delivery.
        if !self.hosted_connection_is_valid().await {
            return 0;
        }
        let n = self.pending.len().min(max);
        buf.reserve(n);
        buf.extend(self.pending.drain(..n).map(|u| u.message));
        n
    }

    async fn hosted_connection_is_valid(&mut self) -> bool {
        let Some(sender) = &self.hosted_sender else { return true };
        let valid = match sender.upgrade() {
            Some(sender) => {
                let valid = match &sender.auth.hosted {
                    Some(proof) if !sender.is_cancelled() => self.offset_supply.check_hosted_auth(proof).await.is_ok(),
                    _ => false,
                };
                if !valid {
                    sender.cancel_hosted_connection();
                }
                valid
            }
            None => false,
        };
        if !valid {
            self.pending.clear();
            self.close();
        }
        valid
    }

    /// Does the next pending update have a tx offset?
    ///
    /// Assumes that [`Self::pending`] is not empty.
    fn pending_update_has_offset(&self) -> bool {
        self.pending.first().is_some_and(|update| update.tx_offset.is_some())
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
    /// The fence installer awaits the returned task's completion before ack.
    pub(crate) fn cancel_hosted_connection(&self) -> AbortHandle {
        self.cancelled.store(true, Ordering::Release);
        self.abort_handle.abort();
        self.abort_handle.clone()
    }

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
            subject: "".into(),
            issuer: "".into(),
            audience: [].into(),
            iat: SystemTime::now(),
            exp: None,
            extra: None,
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
        message: impl Into<OutboundMessage>,
    ) -> Result<(), ClientSendError> {
        let message = message.into();
        debug_assert!(
            matches!(
                (&self.config.version, &message),
                (WsVersion::V1, OutboundMessage::V1(_)) | (WsVersion::V2 | WsVersion::V3, OutboundMessage::V2(_))
            ),
            "attempted to send message variant that does not match client websocket version"
        );
        self.send(ClientUpdate { tx_offset, message })
    }

    fn send(&self, message: ClientUpdate) -> Result<(), ClientSendError> {
        // Do not acquire a database transaction here: broadcasts can already
        // hold one. Durable fencing is checked at admission and delivery.
        if self
            .auth
            .hosted
            .as_ref()
            .is_some_and(|proof| proof.check_at(SystemTime::now()).is_err())
        {
            self.cancel_hosted_connection();
        }
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
                log::warn!(
                    "Client {:?} exceeded channel capacity of {}, kicking",
                    self.id,
                    self.sendtx.capacity(),
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

/// Runs independently of the socket actor, so blocked writes and idle sockets
/// cannot keep credentials alive. Expiry uses a monotonic deadline captured once.
fn spawn_hosted_connection_watchdog(
    sender: std::sync::Weak<ClientConnectionSender>,
    mut supply: impl DurableOffsetSupply + 'static,
    subscriptions: Option<crate::subscription::module_subscription_actor::ModuleSubscriptions>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let actor = sender.upgrade().map(|connection| connection.abort_handle.clone());
        async {
            let Some(connection) = sender.upgrade() else { return };
            let Some(proof) = connection.auth.hosted.clone() else {
                return;
            };
            let deadline = tokio::time::Instant::now() + proof.remaining_lifetime(SystemTime::now());
            drop(connection);
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    biased;
                    _ = tokio::time::sleep_until(deadline) => {
                        if let Some(connection) = sender.upgrade() { connection.cancel_hosted_connection(); }
                        return;
                    }
                    _ = interval.tick() => {}
                }
                let Some(connection) = sender.upgrade() else { return };
                if connection.abort_handle.is_finished() || connection.is_cancelled() {
                    return;
                }
                let checked = tokio::select! {
                    biased;
                    _ = tokio::time::sleep_until(deadline) => {
                        connection.cancel_hosted_connection();
                        return;
                    }
                    checked = supply.check_hosted_auth(&proof) => checked,
                };
                if checked.is_err() {
                    connection.cancel_hosted_connection();
                    return;
                }
            }
        }
        .await;
        // Keep the registry entry until socket I/O has actually stopped. A
        // concurrent target barrier must still find and await an aborted actor.
        if let Some(actor) = actor {
            while !actor.is_finished() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
        if let Some(subscriptions) = subscriptions {
            subscriptions.unregister_hosted_connection(&sender);
        }
    })
}

#[derive(Clone)]
#[non_exhaustive]
pub struct ClientConnection {
    sender: Arc<ClientConnectionSender>,
    pub replica_id: u64,
    module_rx: watch::Receiver<ModuleHost>,
    auth: AuthCtx,
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

/// Wraps the receiving end of an unbounded channel with a gauge for tracking the size of the channel.
/// We subtract the size of the channel from the gauge on drop to avoid leaking the metric.
pub struct MeteredUnboundedReceiver<T> {
    inner: mpsc::UnboundedReceiver<T>,
    gauge: Option<IntGauge>,
}

impl<T> MeteredUnboundedReceiver<T> {
    pub fn new(inner: mpsc::UnboundedReceiver<T>) -> Self {
        Self { inner, gauge: None }
    }

    pub fn with_gauge(inner: mpsc::UnboundedReceiver<T>, gauge: IntGauge) -> Self {
        Self {
            inner,
            gauge: Some(gauge),
        }
    }

    pub async fn recv(&mut self) -> Option<T> {
        poll_fn(|cx| self.poll_recv(cx)).await
    }

    pub fn blocking_recv(&mut self) -> Option<T> {
        self.inner.blocking_recv().inspect(|_| {
            if let Some(gauge) = &self.gauge {
                gauge.dec();
            }
        })
    }

    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let poll = self.inner.poll_recv(cx);
        if let Poll::Ready(Some(_)) = poll
            && let Some(gauge) = &self.gauge
        {
            gauge.dec()
        }
        poll
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

impl<T> Drop for MeteredUnboundedReceiver<T> {
    fn drop(&mut self) {
        // Record the number of elements still in the channel on drop
        if let Some(gauge) = &self.gauge {
            gauge.sub(self.inner.len() as _);
        }
    }
}

/// Wraps the transmitting end of an unbounded channel with a gauge for tracking the size of the channel.
pub struct MeteredUnboundedSender<T> {
    inner: mpsc::UnboundedSender<T>,
    gauge: Option<IntGauge>,
}

impl<T> Clone for MeteredUnboundedSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            gauge: self.gauge.clone(),
        }
    }
}

impl<T> MeteredUnboundedSender<T> {
    pub fn new(inner: mpsc::UnboundedSender<T>) -> Self {
        Self { inner, gauge: None }
    }

    pub fn with_gauge(inner: mpsc::UnboundedSender<T>, gauge: IntGauge) -> Self {
        Self {
            inner,
            gauge: Some(gauge),
        }
    }

    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        if let Some(gauge) = &self.gauge {
            gauge.inc();
        }
        if let Err(err) = self.inner.send(value) {
            if let Some(gauge) = &self.gauge {
                gauge.dec();
            }
            return Err(err);
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
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn<Fut>(
        id: ClientActorId,
        auth: ConnectionAuthCtx,
        sql_auth: AuthCtx,
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
        let mut receiver = ClientConnectionReceiver::new(
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
        if sender.auth.hosted.is_some() {
            receiver.hosted_sender = Some(Arc::downgrade(&sender));
            if module.subscriptions().register_hosted_connection(&sender).is_err() {
                sender.cancel_hosted_connection();
            } else {
                spawn_hosted_connection_watchdog(
                    Arc::downgrade(&sender),
                    module_rx.clone(),
                    Some(module.subscriptions().clone()),
                );
            }
        }
        let this = Self {
            sender,
            replica_id,
            module_rx,
            auth: sql_auth,
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
        Self::dummy_with_receiver(id, config, replica_id, module_rx).0
    }

    pub fn dummy_with_receiver(
        id: ClientActorId,
        config: ClientConfig,
        replica_id: u64,
        module_rx: watch::Receiver<ModuleHost>,
    ) -> (Self, ClientConnectionReceiver) {
        let auth = AuthCtx::new(module_rx.borrow().database_info().database_identity, id.identity);
        let (sender, receiver) = ClientConnectionSender::dummy_with_channel(id, config, module_rx.clone());
        (
            Self {
                sender: Arc::new(sender),
                replica_id,
                module_rx,
                auth,
            },
            receiver,
        )
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
        flags: ws_v1::CallReducerFlags,
    ) -> Result<crate::host::ReducerCallResult, ReducerCallError> {
        let caller = match flags {
            ws_v1::CallReducerFlags::FullUpdate => Some(self.sender()),
            // Setting `sender = None` causes `eval_updates` to skip sending to the caller
            // as it has no access to the caller other than by id/connection id.
            ws_v1::CallReducerFlags::NoSuccessNotify => None,
        };

        self.module()
            .call_reducer(
                &self.sender.auth,
                Some(self.id.connection_id),
                caller,
                Some(request_id),
                Some(timer),
                reducer,
                args,
            )
            .await
    }

    pub async fn call_reducer_v2(
        &self,
        reducer: &str,
        args: Bytes,
        request_id: RequestId,
        timer: Instant,
        _flags: ws_v2::CallReducerFlags,
    ) -> Result<crate::host::ReducerCallResult, ReducerCallError> {
        self.module()
            .call_reducer(
                &self.sender.auth,
                Some(self.id.connection_id),
                Some(self.sender()),
                Some(request_id),
                Some(timer),
                reducer,
                FunctionArgs::Bsatn(args),
            )
            .await
    }

    pub async fn enqueue_reducer(
        &self,
        reducer: &str,
        args: FunctionArgs,
        request_id: RequestId,
        timer: Instant,
        flags: ws_v1::CallReducerFlags,
    ) -> Result<(), ReducerCallError> {
        let caller = match flags {
            ws_v1::CallReducerFlags::FullUpdate => Some(self.sender()),
            ws_v1::CallReducerFlags::NoSuccessNotify => None,
        };

        self.module()
            .enqueue_reducer(
                &self.sender.auth,
                Some(self.id.connection_id),
                caller,
                Some(request_id),
                Some(timer),
                reducer,
                args,
            )
            .await
    }

    pub async fn enqueue_reducer_v2(
        &self,
        reducer: &str,
        args: Bytes,
        request_id: RequestId,
        timer: Instant,
        _flags: ws_v2::CallReducerFlags,
    ) -> Result<(), ReducerCallError> {
        self.module()
            .enqueue_reducer(
                &self.sender.auth,
                Some(self.id.connection_id),
                Some(self.sender()),
                Some(request_id),
                Some(timer),
                reducer,
                FunctionArgs::Bsatn(args),
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
        self.module()
            .enqueue_procedure(
                &self.sender.auth,
                Some(self.id.connection_id),
                Some(timer),
                procedure,
                args,
                ProcedureResultTarget::new(self.sender(), request_id),
            )
            .await
    }

    pub async fn call_procedure_v2(
        &self,
        procedure: &str,
        args: Bytes,
        request_id: RequestId,
        timer: Instant,
        _flags: ws_v2::CallProcedureFlags,
    ) -> Result<(), BroadcastError> {
        self.module()
            .enqueue_procedure(
                &self.sender.auth,
                Some(self.id.connection_id),
                Some(timer),
                procedure,
                FunctionArgs::Bsatn(args),
                ProcedureResultTarget::new(self.sender(), request_id),
            )
            .await
    }

    pub async fn subscribe_single(
        &self,
        subscription: ws_v1::SubscribeSingle,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
        self.module()
            .call_view_add_single_subscription(self.sender(), self.auth.clone(), subscription, timer)
            .await
    }

    pub async fn unsubscribe(
        &self,
        request: ws_v1::Unsubscribe,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
        self.module()
            .call_view_remove_single_subscription(self.sender(), self.auth.clone(), request, timer)
            .await
    }

    pub async fn subscribe_v2(
        &self,
        request: ws_v2::Subscribe,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
        self.module()
            .call_view_add_v2_subscription(self.sender(), self.auth.clone(), request, timer)
            .await
    }
    pub async fn subscribe_multi(
        &self,
        request: ws_v1::SubscribeMulti,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
        self.module()
            .call_view_add_multi_subscription(self.sender(), self.auth.clone(), request, timer)
            .await
    }

    pub async fn unsubscribe_multi(
        &self,
        request: ws_v1::UnsubscribeMulti,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
        self.module()
            .call_view_remove_multi_subscription(self.sender(), self.auth.clone(), request, timer)
            .await
    }

    pub async fn unsubscribe_v2(
        &self,
        request: ws_v2::Unsubscribe,
        timer: Instant,
    ) -> Result<Option<ExecutionMetrics>, DBError> {
        self.module()
            .call_view_remove_v2_subscription(self.sender(), self.auth.clone(), request, timer)
            .await
    }

    pub async fn subscribe(&self, subscription: ws_v1::Subscribe, timer: Instant) -> Result<ExecutionMetrics, DBError> {
        self.module()
            .call_view_add_legacy_subscription(self.sender(), self.auth.clone(), subscription, timer)
            .await
            .map(|metrics| metrics.unwrap_or_default())
    }

    pub async fn one_off_query_json(
        &self,
        query: &str,
        message_id: &[u8],
        timer: Instant,
    ) -> Result<(), anyhow::Error> {
        self.module()
            .one_off_query_json(
                self.auth.clone(),
                query.to_owned(),
                self.sender.clone(),
                message_id.to_owned(),
                timer,
            )
            .await
    }

    pub async fn one_off_query_bsatn(
        &self,
        query: &str,
        message_id: &[u8],
        timer: Instant,
    ) -> Result<(), anyhow::Error> {
        let bsatn_rlb_pool = self.module().replica_ctx().subscriptions.bsatn_rlb_pool.clone();
        self.module()
            .one_off_query_bsatn(
                self.auth.clone(),
                query.to_owned(),
                self.sender.clone(),
                message_id.to_owned(),
                timer,
                bsatn_rlb_pool,
            )
            .await
    }

    pub async fn one_off_query_v2(&self, query: &str, request_id: u32, timer: Instant) -> Result<(), anyhow::Error> {
        let bsatn_rlb_pool = self.module().replica_ctx().subscriptions.bsatn_rlb_pool.clone();
        self.module()
            .one_off_query_v2(
                self.auth.clone(),
                query.to_owned(),
                self.sender.clone(),
                request_id,
                timer,
                bsatn_rlb_pool,
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
    use crate::client::messages::{SerializableMessage, SubscriptionUpdateMessage, TransactionUpdateMessage};

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

    fn empty_tx_update() -> SerializableMessage {
        let msg = TransactionUpdateMessage {
            event: None,
            database_update: SubscriptionUpdateMessage::default_for_protocol(Protocol::Binary, None),
        };
        SerializableMessage::TxUpdate(msg)
    }

    async fn assert_received_update(f: impl Future<Output = Option<OutboundMessage>>) {
        assert_matches!(f.await, Some(OutboundMessage::V1(SerializableMessage::TxUpdate(_))));
    }

    async fn assert_receiver_closed(f: impl Future<Output = Option<OutboundMessage>>) {
        assert_matches!(f.await, None);
    }

    async fn assert_pending(f: &mut (impl Future<Output: fmt::Debug> + Unpin)) {
        assert_matches!(futures::poll!(f), Poll::Pending);
    }

    fn hosted_auth(db: &RelationalDB, lifetime: std::time::Duration) -> ConnectionAuthCtx {
        // These fixtures model an already reconciled receiving host. Tests of
        // startup closure explicitly close the gate after constructing it.
        if !db.hosted_admission().is_open() {
            db.hosted_admission().begin().unwrap().complete().unwrap();
        }
        use crate::auth::{
            hosted_tokens::{sign_hosted_token, HostedTokenBinding, HostedTokenValidator},
            JwtKeys,
        };
        let keys = JwtKeys::generate().unwrap();
        let now = SystemTime::now();
        let binding = HostedTokenBinding {
            source_database: db.database_identity(),
            target_database: db.database_identity(),
            generation: 1,
            grant_revision: 1,
            lease_expires_at: now + std::time::Duration::from_secs(30),
        };
        let token = sign_hosted_token(&keys.private, "platform.test", &binding, now, now + lifetime, "test").unwrap();
        HostedTokenValidator::new([("platform.test".into(), keys.public)])
            .unwrap()
            .validate_token(&token, db.database_identity(), now, |_, _, _| Some(binding))
            .unwrap()
            .into_connection_auth()
            .unwrap()
    }

    fn set_fence(db: &RelationalDB, generation: u64, allowed: bool) {
        use crate::db::deployment::install_container_fence;
        use spacetimedb_datastore::system_tables::StContainerFenceRow;
        db.with_auto_commit(Workload::ForTests, |tx| {
            install_container_fence(
                db,
                tx,
                &StContainerFenceRow {
                    source_identity: db.database_identity().into(),
                    generation,
                    target_grant_revision: 1,
                    target_set_hash: spacetimedb_lib::hash_bytes(b"targets"),
                    allowed,
                },
            )
        })
        .unwrap();
    }

    fn hosted_client(
        db: &RelationalDB,
        supply: impl DurableOffsetSupply + 'static,
        confirmed_reads: bool,
        lifetime: std::time::Duration,
    ) -> (
        Arc<ClientConnectionSender>,
        ClientConnectionReceiver,
        tokio::task::JoinHandle<()>,
    ) {
        let (mut sender, mut receiver) = ClientConnectionSender::dummy_with_channel(
            ClientActorId::for_test(db.database_identity()),
            ClientConfig {
                confirmed_reads,
                ..ClientConfig::for_test()
            },
            supply,
        );
        sender.auth = hosted_auth(db, lifetime);
        let actor = tokio::spawn(std::future::pending());
        sender.abort_handle = actor.abort_handle();
        let sender = Arc::new(sender);
        receiver.hosted_sender = Some(Arc::downgrade(&sender));
        (sender, receiver, actor)
    }

    struct HostedConfirmedSupply {
        db: Arc<RelationalDB>,
        durable: FakeDurableOffset,
        durability_requested: Arc<AtomicBool>,
    }
    impl DurableOffsetSupply for HostedConfirmedSupply {
        fn durable_offset(&mut self) -> Result<Option<DurableOffset>, NoSuchModule> {
            self.durability_requested.store(true, Ordering::Release);
            self.durable.durable_offset()
        }
        fn check_hosted_auth(
            &mut self,
            proof: &VerifiedHostedAuth,
        ) -> futures::future::BoxFuture<'static, anyhow::Result<()>> {
            self.db.check_hosted_auth(proof)
        }
    }

    #[tokio::test]
    async fn hosted_queued_delivery_rechecks_committed_fence_and_preserves_ordinary_identity() {
        let db = crate::db::relational_db::tests_utils::TestDB::in_memory().unwrap();
        set_fence(&db, 1, true);
        let (sender, mut receiver, actor) =
            hosted_client(&db, db.db.clone(), false, std::time::Duration::from_secs(20));
        sender.send_message(None, empty_tx_update()).unwrap();
        set_fence(&db, 2, false);
        assert_receiver_closed(receiver.recv()).await;
        assert!(sender.is_cancelled());
        assert!(actor.await.unwrap_err().is_cancelled());
        let (ordinary, mut ordinary_rx) = default_client(db.db.clone());
        ordinary.send_message(None, empty_tx_update()).unwrap();
        assert_received_update(ordinary_rx.recv()).await;
    }

    #[tokio::test]
    async fn hosted_queued_delivery_rejects_closed_startup_gate_with_unchanged_fence() {
        let db = crate::db::relational_db::tests_utils::TestDB::in_memory().unwrap();
        set_fence(&db, 1, true);
        let (sender, mut receiver, actor) =
            hosted_client(&db, db.db.clone(), false, std::time::Duration::from_secs(20));
        sender.send_message(None, empty_tx_update()).unwrap();
        db.hosted_admission().close();
        assert_receiver_closed(receiver.recv()).await;
        assert!(sender.is_cancelled());
        assert!(actor.await.unwrap_err().is_cancelled());
        let (ordinary, mut ordinary_rx) = default_client(db.db.clone());
        ordinary.send_message(None, empty_tx_update()).unwrap();
        assert_received_update(ordinary_rx.recv()).await;
    }

    #[tokio::test]
    async fn hosted_confirmed_delivery_rechecks_fence_after_durability_wait() {
        let db = crate::db::relational_db::tests_utils::TestDB::in_memory().unwrap();
        set_fence(&db, 1, true);
        let durable = FakeDurableOffset::new();
        let durability_requested = Arc::new(AtomicBool::new(false));
        let supply = HostedConfirmedSupply {
            db: db.db.clone(),
            durable: durable.clone(),
            durability_requested: durability_requested.clone(),
        };
        let (sender, mut receiver, actor) = hosted_client(&db, supply, true, std::time::Duration::from_secs(20));
        sender.send_message(Some(7), empty_tx_update()).unwrap();
        let mut receiving = Box::pin(receiver.recv());
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !durability_requested.load(Ordering::Acquire) {
                assert_pending(&mut receiving).await;
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        set_fence(&db, 2, false);
        durable.mark_durable_at(7);
        assert_receiver_closed(receiving).await;
        assert!(actor.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn hosted_idle_connection_expires_without_outbound_traffic() {
        let db = crate::db::relational_db::tests_utils::TestDB::in_memory().unwrap();
        set_fence(&db, 1, true);
        let (sender, _receiver, actor) = hosted_client(&db, db.db.clone(), false, std::time::Duration::from_secs(2));
        let watchdog = spawn_hosted_connection_watchdog(Arc::downgrade(&sender), db.db.clone(), None);
        tokio::time::timeout(std::time::Duration::from_secs(3), watchdog)
            .await
            .unwrap()
            .unwrap();
        assert!(sender.is_cancelled());
        assert!(actor.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn hosted_connection_uses_confirmed_expiry_while_authority_read_is_blocked() {
        struct BlockedAuthority;
        impl DurableOffsetSupply for BlockedAuthority {
            fn durable_offset(&mut self) -> Result<Option<DurableOffset>, NoSuchModule> {
                Ok(None)
            }

            fn check_hosted_auth(
                &mut self,
                _: &VerifiedHostedAuth,
            ) -> futures::future::BoxFuture<'static, anyhow::Result<()>> {
                Box::pin(std::future::pending())
            }
        }

        let db = crate::db::relational_db::tests_utils::TestDB::in_memory().unwrap();
        set_fence(&db, 1, true);
        let (mut sender, _receiver) = ClientConnectionSender::dummy_with_channel(
            ClientActorId::for_test(db.database_identity()),
            ClientConfig::for_test(),
            db.db.clone(),
        );
        let proof = hosted_auth(&db, std::time::Duration::from_secs(20)).hosted.unwrap();
        let confirmed_time = proof.expires_at() - std::time::Duration::from_secs(5);
        // Authority is ahead of the receiving wall clock. Almost all of the
        // five remaining seconds elapsed while its confirmation was in flight.
        let started = std::time::Instant::now() - std::time::Duration::from_millis(4_900);
        sender.auth = proof
            .constrain_expiration(confirmed_time, started)
            .unwrap()
            .into_connection_auth()
            .unwrap();
        let actor = tokio::spawn(std::future::pending::<()>());
        sender.abort_handle = actor.abort_handle();
        let sender = Arc::new(sender);
        let watchdog = spawn_hosted_connection_watchdog(Arc::downgrade(&sender), BlockedAuthority, None);
        tokio::time::timeout(std::time::Duration::from_secs(2), watchdog)
            .await
            .unwrap()
            .unwrap();
        assert!(sender.is_cancelled());
        assert!(actor.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn hosted_idle_connection_rechecks_durable_revocation() {
        let db = crate::db::relational_db::tests_utils::TestDB::in_memory().unwrap();
        set_fence(&db, 1, true);
        let (sender, _receiver, actor) = hosted_client(&db, db.db.clone(), false, std::time::Duration::from_secs(20));
        let watchdog = spawn_hosted_connection_watchdog(Arc::downgrade(&sender), db.db.clone(), None);
        set_fence(&db, 2, false);
        tokio::time::timeout(std::time::Duration::from_secs(2), watchdog)
            .await
            .unwrap()
            .unwrap();
        assert!(sender.is_cancelled());
        assert!(actor.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn hosted_watchdog_ends_and_unregisters_after_socket_actor_finishes() {
        use crate::subscription::module_subscription_actor::ModuleSubscriptions;
        let db = crate::db::relational_db::tests_utils::TestDB::in_memory().unwrap();
        set_fence(&db, 1, true);
        let subscriptions = ModuleSubscriptions::for_test_enclosing_runtime(db.db.clone());
        let (sender, _receiver, actor) = hosted_client(&db, db.db.clone(), false, std::time::Duration::from_secs(20));
        subscriptions.register_hosted_connection(&sender).unwrap();
        assert_eq!(subscriptions.hosted_connection_count(), 1);
        let watchdog =
            spawn_hosted_connection_watchdog(Arc::downgrade(&sender), db.db.clone(), Some(subscriptions.clone()));
        actor.abort();
        let _ = actor.await;
        tokio::time::timeout(std::time::Duration::from_secs(2), watchdog)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(subscriptions.hosted_connection_count(), 0);
        // The registry releases the entry even while another owner retains sender.
        assert!(!sender.is_cancelled());
    }

    #[tokio::test]
    async fn hosted_registry_retains_cancelled_actor_until_delivery_cleanup_finishes() {
        use crate::subscription::module_subscription_actor::ModuleSubscriptions;
        let db = crate::db::relational_db::tests_utils::TestDB::in_memory().unwrap();
        set_fence(&db, 1, true);
        let subscriptions = ModuleSubscriptions::for_test_enclosing_runtime(db.db.clone());
        let (mut sender, _receiver) = default_client(db.db.clone());
        sender.auth = hosted_auth(&db, std::time::Duration::from_secs(20));
        let (release, blocked) = std::sync::mpsc::channel();
        let (started, ready) = oneshot::channel();
        // A started blocking task models cleanup that cannot complete merely
        // because abort was requested. The barrier must retain its handle.
        let actor = tokio::task::spawn_blocking(move || {
            let _ = started.send(());
            let _ = blocked.recv();
        });
        ready.await.unwrap();
        sender.abort_handle = actor.abort_handle();
        let sender = Arc::new(sender);
        subscriptions.register_hosted_connection(&sender).unwrap();
        let watchdog =
            spawn_hosted_connection_watchdog(Arc::downgrade(&sender), db.db.clone(), Some(subscriptions.clone()));
        set_fence(&db, 2, false);
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !sender.is_cancelled() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(subscriptions.hosted_connection_count(), 1);
        let handles = db.with_read_only(Workload::ForTests, |tx| {
            subscriptions.cancel_invalid_hosted_connections(tx)
        });
        assert_eq!(handles.len(), 1);
        assert!(!handles[0].is_finished());
        release.send(()).unwrap();
        actor.await.unwrap();
        watchdog.await.unwrap();
        assert!(handles[0].is_finished());
        assert_eq!(subscriptions.hosted_connection_count(), 0);
    }

    #[tokio::test]
    async fn hosted_target_barrier_cancels_connections_without_subscriptions_and_waits_for_actor() {
        use crate::db::deployment::install_container_fence;
        use crate::subscription::module_subscription_actor::ModuleSubscriptions;
        use spacetimedb_datastore::system_tables::StContainerFenceRow;
        let db = crate::db::relational_db::tests_utils::TestDB::in_memory().unwrap();
        set_fence(&db, 1, true);
        let subscriptions = ModuleSubscriptions::for_test_enclosing_runtime(db.db.clone());
        let (sender, _receiver, actor) = hosted_client(&db, db.db.clone(), false, std::time::Duration::from_secs(20));
        subscriptions.register_hosted_connection(&sender).unwrap();
        let handles = db
            .with_auto_commit(Workload::ForTests, |tx| {
                install_container_fence(
                    &db,
                    tx,
                    &StContainerFenceRow {
                        source_identity: db.database_identity().into(),
                        generation: 2,
                        target_grant_revision: 1,
                        target_set_hash: spacetimedb_lib::hash_bytes(b"targets"),
                        allowed: true,
                    },
                )?;
                Ok::<_, anyhow::Error>(subscriptions.cancel_invalid_hosted_connections(tx))
            })
            .unwrap();
        assert_eq!(handles.len(), 1);
        assert!(actor.await.unwrap_err().is_cancelled());
        assert!(handles[0].is_finished());
        assert!(subscriptions.register_hosted_connection(&sender).is_err());
    }

    #[tokio::test]
    async fn hosted_subscription_rejected_under_transaction_before_query_compilation() {
        use crate::subscription::module_subscription_actor::ModuleSubscriptions;
        let db = crate::db::relational_db::tests_utils::TestDB::in_memory().unwrap();
        let subscriptions = ModuleSubscriptions::for_test_enclosing_runtime(db.db.clone());
        let (sender, _receiver, actor) = hosted_client(&db, db.db.clone(), false, std::time::Duration::from_secs(20));
        // There is deliberately no installed fence. The malformed SQL verifies
        // authentication fails before query parsing or view materialization.
        let result = subscriptions
            .add_legacy_subscriber(
                None,
                sender.clone(),
                AuthCtx::new(db.database_identity(), db.database_identity()),
                ws_v1::Subscribe {
                    query_strings: ["invalid SQL".into()].into(),
                    request_id: 0,
                },
                Instant::now(),
                None,
            )
            .await;
        assert!(result.unwrap_err().to_string().contains("container"));
        sender.cancel_hosted_connection();
        let _ = actor.await;
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
