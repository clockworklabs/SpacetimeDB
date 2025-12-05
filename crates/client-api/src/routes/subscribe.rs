use std::fmt::Display;
use std::future::{poll_fn, Future};
use std::num::NonZeroUsize;
use std::panic;
use std::pin::{pin, Pin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_stream::stream;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Extension;
use axum_extra::TypedHeader;
use bytes::Bytes;
use bytestring::ByteString;
use derive_more::From;
use futures::{pin_mut, Sink, SinkExt, Stream, StreamExt};
use http::{HeaderValue, StatusCode};
use prometheus::IntGauge;
use scopeguard::{defer, ScopeGuard};
use serde::Deserialize;
use spacetimedb::client::messages::{
    serialize, IdentityTokenMessage, SerializableMessage, SerializeBuffer, SwitchedServerMessage, ToProtocol,
};
use spacetimedb::client::{
    ClientActorId, ClientConfig, ClientConnection, ClientConnectionReceiver, DataMessage, MessageExecutionError,
    MessageHandleError, MeteredReceiver, MeteredSender, Protocol,
};
use spacetimedb::host::module_host::ClientConnectedError;
use spacetimedb::host::NoSuchModule;
use spacetimedb::util::spawn_rayon;
use spacetimedb::worker_metrics::WORKER_METRICS;
use spacetimedb::Identity;
use spacetimedb_client_api_messages::websocket::{self as ws_api, Compression};
use spacetimedb_datastore::execution_context::WorkloadType;
use spacetimedb_lib::connection_id::{ConnectionId, ConnectionIdForUrl};
use std::time::Instant;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::error::Elapsed;
use tokio::time::{sleep_until, timeout};
use tokio_tungstenite::tungstenite::Utf8Bytes;

use crate::auth::SpacetimeAuth;
use crate::util::serde::humantime_duration;
use crate::util::websocket::{
    CloseCode, CloseFrame, Message as WsMessage, WebSocketConfig, WebSocketStream, WebSocketUpgrade, WsError,
};
use crate::util::{NameOrIdentity, XForwardedFor};
use crate::{log_and_500, Authorization, ControlStateDelegate, NodeDelegate};

#[allow(clippy::declare_interior_mutable_const)]
pub const TEXT_PROTOCOL: HeaderValue = HeaderValue::from_static(ws_api::TEXT_PROTOCOL);
#[allow(clippy::declare_interior_mutable_const)]
pub const BIN_PROTOCOL: HeaderValue = HeaderValue::from_static(ws_api::BIN_PROTOCOL);

pub trait HasWebSocketOptions {
    fn websocket_options(&self) -> WebSocketOptions;
}

impl<T: HasWebSocketOptions> HasWebSocketOptions for Arc<T> {
    fn websocket_options(&self) -> WebSocketOptions {
        (**self).websocket_options()
    }
}

#[derive(Deserialize)]
pub struct SubscribeParams {
    pub name_or_identity: NameOrIdentity,
}

#[derive(Deserialize)]
pub struct SubscribeQueryParams {
    pub connection_id: Option<ConnectionIdForUrl>,
    #[serde(default)]
    pub compression: Compression,
    /// Whether we want "light" responses, tailored to network bandwidth constrained clients.
    /// This knob works by setting other, more specific, knobs to the value.
    #[serde(default)]
    pub light: bool,
    /// If `true`, send the subscription updates only after the transaction
    /// offset they're computed from is confirmed to be durable.
    ///
    /// If `false`, send them immediately.
    #[serde(default)]
    pub confirmed: bool,
}

pub fn generate_random_connection_id() -> ConnectionId {
    ConnectionId::from_le_byte_array(rand::random())
}

pub async fn handle_websocket<S>(
    State(ctx): State<S>,
    Path(SubscribeParams { name_or_identity }): Path<SubscribeParams>,
    Query(SubscribeQueryParams {
        connection_id,
        compression,
        light,
        confirmed,
    }): Query<SubscribeQueryParams>,
    forwarded_for: Option<TypedHeader<XForwardedFor>>,
    Extension(auth): Extension<SpacetimeAuth>,
    ws: WebSocketUpgrade,
) -> axum::response::Result<impl IntoResponse>
where
    S: NodeDelegate + ControlStateDelegate + HasWebSocketOptions + Authorization,
{
    if connection_id.is_some() {
        // TODO: Bump this up to `log::warn!` after removing the client SDKs' uses of that parameter.
        log::debug!("The connection_id query parameter to the subscribe HTTP endpoint is internal and will be removed in a future version of SpacetimeDB.");
    }

    let connection_id = connection_id
        .map(ConnectionId::from)
        .unwrap_or_else(generate_random_connection_id);

    if connection_id == ConnectionId::ZERO {
        Err((
            StatusCode::BAD_REQUEST,
            "Invalid connection ID: the all-zeros ConnectionId is reserved.",
        ))?;
    }

    let db_identity = name_or_identity.resolve(&ctx).await?;
    let sql_auth = ctx.authorize_sql(auth.claims.identity, db_identity).await?;

    let (res, ws_upgrade, protocol) =
        ws.select_protocol([(BIN_PROTOCOL, Protocol::Binary), (TEXT_PROTOCOL, Protocol::Text)]);

    let protocol = protocol.ok_or((StatusCode::BAD_REQUEST, "no valid protocol selected"))?;
    let client_config = ClientConfig {
        protocol,
        compression,
        tx_update_full: !light,
        confirmed_reads: confirmed,
    };

    // TODO: Should also maybe refactor the code and the protocol to allow a single websocket
    // to connect to multiple modules

    let database = ctx
        .get_database_by_identity(&db_identity)
        .await
        .unwrap()
        .ok_or(StatusCode::NOT_FOUND)?;

    let leader = ctx
        .leader(database.id)
        .await
        .map_err(log_and_500)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let identity_token = auth.creds.token().into();

    let mut module_rx = leader.module_watcher().await.map_err(log_and_500)?;

    let client_identity = auth.claims.identity;
    let client_id = ClientActorId {
        identity: client_identity,
        connection_id,
        name: ctx.client_actor_index().next_client_name(),
    };

    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(0x2000000))
        .max_frame_size(None)
        .accept_unmasked_frames(false);
    let ws_opts = ctx.websocket_options();

    tokio::spawn(async move {
        let ws = match ws_upgrade.upgrade(ws_config).await {
            Ok(ws) => ws,
            Err(err) => {
                log::error!("websocket: WebSocket init error: {err}");
                return;
            }
        };

        let identity = client_id.identity;
        let client_log_string = match forwarded_for {
            Some(TypedHeader(XForwardedFor(ip))) => {
                format!("ip {ip} with Identity {identity} and ConnectionId {connection_id}")
            }
            None => format!("unknown ip with Identity {identity} and ConnectionId {connection_id}"),
        };

        log::debug!("websocket: New client connected from {client_log_string}");

        let connected = match ClientConnection::call_client_connected_maybe_reject(
            &mut module_rx,
            client_id,
            auth.clone().into(),
        )
        .await
        {
            Ok(connected) => {
                log::debug!("websocket: client_connected returned Ok for {client_log_string}");
                connected
            }
            Err(e @ (ClientConnectedError::Rejected(_) | ClientConnectedError::OutOfEnergy)) => {
                log::info!(
                    "websocket: Rejecting connection for {client_log_string} due to error from client_connected reducer: {e}"
                );
                return;
            }
            Err(e @ (ClientConnectedError::DBError(_) | ClientConnectedError::ReducerCall(_))) => {
                log::warn!("websocket: ModuleHost died while {client_log_string} was connecting: {e:#}");
                return;
            }
        };

        log::debug!(
            "websocket: Database accepted connection from {client_log_string}; spawning ws_client_actor and ClientConnection"
        );

        let actor = |client, receiver| ws_client_actor(ws_opts, client, ws, receiver);
        let client = ClientConnection::spawn(
            client_id,
            auth.into(),
            sql_auth,
            client_config,
            leader.replica_id,
            module_rx,
            actor,
            connected,
        )
        .await;

        // Send the client their identity token message as the first message
        // NOTE: We're adding this to the protocol because some client libraries are
        // unable to access the http response headers.
        // Clients that receive the token from the response headers should ignore this
        // message.
        let message = IdentityTokenMessage {
            identity: client_identity,
            token: identity_token,
            connection_id,
        };
        if let Err(e) = client.send_message(None, message) {
            log::warn!("websocket: Error sending IdentityToken message to {client_log_string}: {e}");
        }
    });

    Ok(res)
}

struct ActorState {
    pub client_id: ClientActorId,
    pub database: Identity,
    config: WebSocketOptions,
    closed: AtomicBool,
    got_pong: AtomicBool,
}

impl ActorState {
    pub fn new(database: Identity, client_id: ClientActorId, config: WebSocketOptions) -> Self {
        Self {
            database,
            client_id,
            config,
            closed: AtomicBool::new(false),
            got_pong: AtomicBool::new(true),
        }
    }

    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    pub fn close(&self) -> bool {
        self.closed.swap(true, Ordering::Relaxed)
    }

    pub fn set_ponged(&self) {
        self.got_pong.store(true, Ordering::Relaxed);
    }

    pub fn reset_ponged(&self) -> bool {
        self.got_pong.swap(false, Ordering::Relaxed)
    }

    pub fn next_idle_deadline(&self) -> Instant {
        Instant::now() + self.config.idle_timeout
    }
}

/// Configuration for WebSocket connections.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct WebSocketOptions {
    /// Interval at which to send `Ping` frames.
    ///
    /// We use pings for connection keep-alive.
    /// Value must be smaller than `idle_timeout`.
    ///
    /// Default: 15s
    #[serde(with = "humantime_duration")]
    #[serde(default = "WebSocketOptions::default_ping_interval")]
    pub ping_interval: Duration,
    /// Amount of time after which an idle connection is closed.
    ///
    /// A connection is considered idle if no data is received nor sent.
    /// This includes `Ping`/`Pong` frames used for keep-alive.
    ///
    /// Value must be greater than `ping_interval`.
    ///
    /// Default: 30s
    #[serde(with = "humantime_duration")]
    #[serde(default = "WebSocketOptions::default_idle_timeout")]
    pub idle_timeout: Duration,
    /// For how long to keep draining the incoming messages until a client close
    /// is received.
    ///
    /// Default: 250ms
    #[serde(with = "humantime_duration")]
    #[serde(default = "WebSocketOptions::default_close_handshake_timeout")]
    pub close_handshake_timeout: Duration,
    /// Maximum number of messages to queue for processing.
    ///
    /// If this number is exceeded, the client is disconnected.
    ///
    /// Default: 16384
    #[serde(default = "WebSocketOptions::default_incoming_queue_length")]
    pub incoming_queue_length: NonZeroUsize,
}

impl Default for WebSocketOptions {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl WebSocketOptions {
    const DEFAULT_PING_INTERVAL: Duration = Duration::from_secs(15);
    const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
    const DEFAULT_CLOSE_HANDSHAKE_TIMEOUT: Duration = Duration::from_millis(250);
    const DEFAULT_INCOMING_QUEUE_LENGTH: NonZeroUsize = NonZeroUsize::new(16384).expect("16384 > 0, qed");

    const DEFAULT: Self = Self {
        ping_interval: Self::DEFAULT_PING_INTERVAL,
        idle_timeout: Self::DEFAULT_IDLE_TIMEOUT,
        close_handshake_timeout: Self::DEFAULT_CLOSE_HANDSHAKE_TIMEOUT,
        incoming_queue_length: Self::DEFAULT_INCOMING_QUEUE_LENGTH,
    };

    const fn default_ping_interval() -> Duration {
        Self::DEFAULT_PING_INTERVAL
    }

    const fn default_idle_timeout() -> Duration {
        Self::DEFAULT_IDLE_TIMEOUT
    }

    const fn default_close_handshake_timeout() -> Duration {
        Self::DEFAULT_CLOSE_HANDSHAKE_TIMEOUT
    }

    const fn default_incoming_queue_length() -> NonZeroUsize {
        Self::DEFAULT_INCOMING_QUEUE_LENGTH
    }
}

async fn ws_client_actor(
    options: WebSocketOptions,
    client: ClientConnection,
    ws: WebSocketStream,
    sendrx: ClientConnectionReceiver,
) {
    // ensure that even if this task gets cancelled, we always cleanup the connection
    let mut client = scopeguard::guard(client, |client| {
        tokio::spawn(client.disconnect());
    });

    ws_client_actor_inner(&mut client, options, ws, sendrx).await;

    ScopeGuard::into_inner(client).disconnect().await;
}

async fn ws_client_actor_inner(
    client: &mut ClientConnection,
    config: WebSocketOptions,
    ws: WebSocketStream,
    sendrx: ClientConnectionReceiver,
) {
    let database = client.module().info().database_identity;
    let client_id = client.id;
    let client_closed_metric = WORKER_METRICS.ws_clients_closed_connection.with_label_values(&database);
    let state = Arc::new(ActorState::new(database, client_id, config));

    // Channel for [`UnorderedWsMessage`]s.
    let (unordered_tx, unordered_rx) = mpsc::unbounded_channel();

    // Split websocket into send and receive halves.
    let (ws_send, ws_recv) = ws.split();

    // Set up the idle timer.
    let (idle_tx, idle_rx) = watch::channel(state.next_idle_deadline());
    let idle_timer = ws_idle_timer(idle_rx);

    // Spawn a task to send outgoing messages
    // obtained from `sendrx` and `unordered_rx`.
    let send_task = tokio::spawn(ws_send_loop(
        state.clone(),
        client.config,
        ws_send,
        sendrx,
        unordered_rx,
    ));
    // Spawn a task to handle incoming messages.
    let recv_task = tokio::spawn(ws_recv_task(
        state.clone(),
        idle_tx,
        client_closed_metric,
        {
            let client = client.clone();
            move |data, timer| {
                let client = client.clone();
                async move { client.handle_message(data, timer).await }
            }
        },
        unordered_tx.clone(),
        ws_recv,
    ));
    let hotswap = {
        let client = client.clone();
        move || {
            let mut client = client.clone();
            async move { client.watch_module_host().await }
        }
    };

    ws_main_loop(state, hotswap, idle_timer, send_task, recv_task, move |msg| {
        let _ = unordered_tx.send(msg);
    })
    .await;
    log::info!("Client connection ended: {client_id}");
}

/// The main `select!` loop of the websocket client actor.
///
/// > This function is defined standalone with generic parameters so that its
/// > behavior can be tested in isolation, not requiring I/O and allowing to
/// > mock effects easily.
///
/// The loop's responsibilities are:
///
/// - Drive the tasks handling the send and receive ends of the websockets to
///   completion, terminating when either of them completes.
///
/// - Terminating if the connection is idle for longer than [`ActorConfig::idle_timeout`].
///   The connection becomes idle if nothing is received from the socket.
///
/// - Periodically sending `Ping` frames to prevent the connection from becoming
///   idle (the client is supposed to respond with `Pong`, which resets the
///   idle timer). See [`ActorConfig::ping_interval`].
///
/// - Watch for changes to the [`ClientConnection`]'s module reference.
///   If it changes, the [`ClientConnection`] "hotswaps" the module, if it
///   is exited, the loop schedules a `Close` frame to be sent, initiating a
///   connection shutdown.
///
/// A peculiarity of handling termination is the websocket [close handshake]:
/// whichever side wants to close the connection sends a `Close` frame and needs
/// to wait for the other end to respond with a `Close` for the connection to
/// end cleanly.
///
/// `tungstenite` handles the protocol details of the close handshake for us,
/// but for it to work properly, we must keep polling the socket until the
/// handshake is complete.
///
/// This is straightforward when the client initiates the close, as the receive
/// stream will just become exhausted, and we'll exit the loop.
///
/// In the case of a server-initiated close, it's a bit more tricky, as we're
/// not supposed to send any more data after a `Close` frame (and `tungstenite`
/// prevents it). Yet, we need to keep polling the receive end until either
/// the `Close` response (which could be queued behind a large number of
/// outstanding messages) arrives, or a timeout elapses (in case the client
/// never responds).
///
/// The implementations [`ws_recv_loop`] and [`ws_send_loop`] thus share the
/// [`ActorState`], which tracks whether the connection is in the closing phase
/// ([`ActorState::closed()`]). If closed, both the send and receive loops keep
/// running, but drop any incoming or outgoing messages respectively until
/// either the `Close` response arrives or [`ActorConfig::close_handshake_timeout`]
/// elapses.
///
///
/// Parameters:
///
/// * **state**:
///   The shared [`ActorState`], updated here when a `Pong` message is received.
///
/// * **hotswap**:
///   An abstraction for [`ClientConnection::watch_module_host`], which updates
///   the connection's internal reference to the module if it was updated,
///   allowing database updates without disconnecting clients.
///
///   It is polled here for its error return value: if the output of the future
///   is `Err(NoSuchModule)`, the database was shut down and existing clients
///   must be disconnected.
///
/// * **idle_timer**:
///   Abstraction for [`ws_idle_timer`]: if and when the future completes, the
///   connection is considered unresponsive, and the connection is closed.
///
///   The idle timer should be reset whenever data is received from the websocket.
///
/// * **send_task**:
///   Task handling outgoing messages. Holds the receive end of `unordered_tx`.
///
///   If the task returns, the connection is considered bad, and the main loop
///   exits. If the task panicked, the panic is resumed on the current thread.
///
///   Note that the send task must not terminate after it has sent a `Close`
///   frame (via `unordered_tx`) -- the websocket protocol mandates that the
///   initiator of the close handshake wait for the other end to respond with
///   a `Close` frame. Thus, the loop must continue to poll `recv_task` and not
///   exit due to `send_task` being complete.
///
///   See [`ws_send_loop`].
///
/// * **recv_task**:
///   Task handling incoming messages.
///
///   If the task returns, the connection is considered closed, and the main
///   loop exits. If the task panicked, the panic is resumed on the current
///   thread.
///
///   See [`ws_recv_task`].
///
/// * **unordered_tx**:
///   Channel connected to `send_task` that allows the loop to send `Ping` and
///   `Close` frames.
///
///   Note that messages sent while the receiving `send_task` is already
///   terminated are silently ignored. This is safe because the loop will exit
///   anyway when the `send_task` is complete.
///
///
/// [close handshake]: https://datatracker.ietf.org/doc/html/rfc6455#section-7
async fn ws_main_loop<HotswapWatcher>(
    state: Arc<ActorState>,
    hotswap: impl Fn() -> HotswapWatcher,
    idle_timer: impl Future<Output = ()>,
    mut send_task: JoinHandle<()>,
    mut recv_task: JoinHandle<()>,
    unordered_tx: impl Fn(UnorderedWsMessage),
) where
    HotswapWatcher: Future<Output = Result<(), NoSuchModule>>,
{
    // Ensure we terminate both tasks if either exits.
    let abort_send = send_task.abort_handle();
    let abort_recv = recv_task.abort_handle();
    defer! {
        abort_send.abort();
        abort_recv.abort();
    };
    // Set up the ping interval.
    let mut ping_interval = tokio::time::interval(state.config.ping_interval);
    // Arm the first hotswap watcher.
    let watch_hotswap = hotswap();

    pin_mut!(watch_hotswap);
    pin_mut!(idle_timer);

    loop {
        let closed = state.closed();

        tokio::select! {
            // Drive send and receive tasks to completion,
            // propagating panics.
            //
            // If either task completes,
            // the connection is considered closed and we break the loop.
            //
            // NOTE: We don't abort the tasks until this function returns,
            // so the `Err` can't contain an `is_cancelled()` value.
            //
            // Even if the tasks were cancelled (e.g. if the caller retains
            // [`tokio::task::AbortHandle`]s), the reasonable thing to do is to
            // exit the loop as if the tasks completed normally.
            res = &mut send_task => {
                if let Err(e) = res
                    && e.is_panic() {
                        panic::resume_unwind(e.into_panic())
                    }
                break;
            },
            res = &mut recv_task => {
                if let Err(e) = res
                    && e.is_panic() {
                        panic::resume_unwind(e.into_panic())
                    }
                break;
            },

            // Exit if we haven't heard from the client for too long.
            _ = &mut idle_timer => {
                log::warn!("Client {} timed out", state.client_id);
                break;
            },

            // Update the client's module host if it was hotswapped,
            // or close the session if the module exited.
            //
            // Branch is disabled if we already sent a close frame.
            res = &mut watch_hotswap, if !closed => {
                if let Err(NoSuchModule) = res {
                    let close = CloseFrame {
                        code: CloseCode::Away,
                        reason: "module exited".into()
                    };
                    unordered_tx(close.into());
                }
                watch_hotswap.set(hotswap());
            },

            // Send ping.
            //
            // If we didn't receive a response to the last ping,
            // we don't bother sending a fresh one.
            //
            // Either the connection is idle (in which case the timer will kick
            // in), or there is a massive backlog to process until the pong
            // appears on the ordered stream. In either case, adding more pings
            // is of no value.
            //
            // Branch is disabled if we already sent a close frame.
            _ = ping_interval.tick(), if !closed => {
                let was_ponged = state.reset_ponged();
                if was_ponged {
                    unordered_tx(UnorderedWsMessage::Ping(Bytes::new()));
                }
            }
        }
    }
}

/// A sleep that can be extended by sending it new deadlines.
///
/// Sleeps until the deadline appearing on the `activity` channel,
/// i.e. if a new deadline appears before the sleep finishes,
/// the sleep is reset to the new deadline.
///
/// The `activity` should be updated whenever a new message is received.
async fn ws_idle_timer(mut activity: watch::Receiver<Instant>) {
    let mut deadline = *activity.borrow();
    let sleep = sleep_until(deadline.into());
    pin_mut!(sleep);

    loop {
        tokio::select! {
            biased;

            Ok(()) = activity.changed() => {
                let new_deadline = *activity.borrow_and_update();
                if new_deadline != deadline {
                    deadline = new_deadline;
                    sleep.as_mut().reset(deadline.into());
                }
            },

            () = &mut sleep => {
                break;
            },
        }
    }
}

/// Consumes `ws` by composing [`ws_recv_queue`], [`ws_recv_loop`],
/// [`ws_client_message_handler`] and `message_handler`.
///
/// `idle_tx` is the sending end of a [`ws_idle_timer`]. The [`ws_recv_loop`]
/// sends a new, extended deadline whenever it receives a message.
///
/// `unordered_tx` is used to send message execution errors
/// or to initiate a close handshake.
///
/// Initiates a close handshake if the `message_handler` returns any variant
/// of [`MessageHandleError`] that is **not** [`MessageHandleError::Execution`].
///
/// Terminates if:
///
/// - the `ws` stream is exhausted
/// - or, `unordered_tx` is already closed
///
/// In the latter case, we assume that the connection is in an errored state,
/// such that we wouldn't be able to receive any more messages anyway.
async fn ws_recv_task<MessageHandler>(
    state: Arc<ActorState>,
    idle_tx: watch::Sender<Instant>,
    client_closed_metric: IntGauge,
    message_handler: impl Fn(DataMessage, Instant) -> MessageHandler,
    unordered_tx: mpsc::UnboundedSender<UnorderedWsMessage>,
    ws: impl Stream<Item = Result<WsMessage, WsError>> + Unpin + Send + 'static,
) where
    MessageHandler: Future<Output = Result<(), MessageHandleError>>,
{
    let recv_queue_gauge = WORKER_METRICS
        .total_incoming_queue_length
        .with_label_values(&state.database);
    let recv_queue = ws_recv_queue(state.clone(), unordered_tx.clone(), recv_queue_gauge, ws);
    let recv_loop = pin!(ws_recv_loop(state.clone(), idle_tx, recv_queue));
    let recv_handler = ws_client_message_handler(state.clone(), client_closed_metric, recv_loop);
    pin_mut!(recv_handler);

    while let Some((data, timer)) = recv_handler.next().await {
        let result = message_handler(data, timer).await;
        if let Err(e) = result {
            if let MessageHandleError::Execution(err) = e {
                log::error!("{err:#}");
                // If the send task has exited, also exit this recv task.
                if unordered_tx.send(err.into()).is_err() {
                    break;
                }
                continue;
            }
            log::debug!("Client caused error: {e}");
            let close = CloseFrame {
                code: CloseCode::Error,
                reason: format!("{e:#}").into(),
            };
            // If the send task has exited, also exit this recv task.
            // No need to send the close handshake in that case; the client is already gone.
            if unordered_tx.send(close.into()).is_err() {
                break;
            };
        }
    }
}

/// Stream that consumes a stream of [`WsMessage`]s and yields [`ClientMessage`]s.
///
/// Terminates if:
///
/// - the input stream is exhausted
/// - the input stream yields an error
///
/// If `state.closed`, continues to poll the input stream in order for the
/// websocket close handshake to complete. Any messages received while in this
/// state are dropped.
fn ws_recv_loop(
    state: Arc<ActorState>,
    idle_tx: watch::Sender<Instant>,
    mut ws: impl Stream<Item = Result<WsMessage, WsError>> + Unpin,
) -> impl Stream<Item = ClientMessage> {
    // Get the next message from `ws`, or `None` if the stream is exhausted.
    //
    // If `state.closed`, `ws` is drained until it either yields an `Err`, is
    // exhausted, or a timeout of 250ms has elapsed.
    async fn next_message(
        state: &ActorState,
        ws: &mut (impl Stream<Item = Result<WsMessage, WsError>> + Unpin),
    ) -> Option<Result<WsMessage, WsError>> {
        if state.closed() {
            log::trace!("drain websocket waiting for client close");
            let res: Result<Option<Result<WsMessage, WsError>>, Elapsed> =
                timeout(state.config.close_handshake_timeout, async {
                    while let Some(item) = ws.next().await {
                        match item {
                            Ok(message) => drop(message),
                            Err(e) => return Some(Err(e)),
                        }
                    }
                    None
                })
                .await;
            match res {
                Err(_elapsed) => {
                    log::warn!("timeout waiting for client close");
                    None
                }
                Ok(item) => item, // either error or `None`
            }
        } else {
            log::trace!("await next client message without timeout");
            ws.next().await
        }
    }

    stream! {
        loop {
            let Some(res) = next_message(&state, &mut ws).await else {
                log::trace!("recv stream exhausted");
                break;
            };
            match res {
                Ok(m) => {
                    idle_tx.send(state.next_idle_deadline()).ok();

                    if !state.closed() {
                        yield ClientMessage::from_message(m);
                    }
                    // If closed, keep polling until either:
                    //
                    // - the client sends a close frame (`ws` returns `None)
                    // - or `ws` yields an error
                    log::trace!("message received while already closed");
                }
                // None of the error cases can be meaningfully recovered from
                // (and some can't even occur on the `ws` stream).
                // Exit here but spell out an exhaustive match
                // in order to bring any future library changes to our attention.
                Err(e) => match e {
                    e @ (WsError::ConnectionClosed
                    | WsError::AlreadyClosed
                    | WsError::Io(_)
                    | WsError::Tls(_)
                    | WsError::Capacity(_)
                    | WsError::Protocol(_)
                    | WsError::WriteBufferFull(_)
                    | WsError::Utf8(_)
                    | WsError::AttackAttempt
                    | WsError::Url(_)
                    | WsError::Http(_)
                    | WsError::HttpFormat(_)) => {
                        log::warn!("Websocket receive error: {e}");
                        break;
                    }
                },
            }
        }
    }
}

/// Consumes `ws` and queues its items in a channel.
///
/// The channel is initialized with [`ActorConfig::incoming_queue_length`].
/// If it is at capacity, a connection shutdown is initiated by sending
/// [`UnorderedWsMessage::Close`] via `unordered_tx`.
///
/// Returns the channel receiver.
///
/// NOTE: This function is provided for backwards-compatibility, in particular
/// SDK clients not handling backpressure gracefully, and for observability of
/// transaction backlogging. It will probably go away in the future, see [#1851].
///
/// [#1851]: https://github.com/clockworklabs/SpacetimeDBPrivate/issues/1851
fn ws_recv_queue(
    state: Arc<ActorState>,
    unordered_tx: mpsc::UnboundedSender<UnorderedWsMessage>,
    recv_queue_gauge: IntGauge,
    mut ws: impl Stream<Item = Result<WsMessage, WsError>> + Unpin + Send + 'static,
) -> impl Stream<Item = Result<WsMessage, WsError>> {
    const CLOSE: UnorderedWsMessage = UnorderedWsMessage::Close(CloseFrame {
        code: CloseCode::Again,
        reason: Utf8Bytes::from_static("too many requests"),
    });
    let on_message_after_close = move |client_id| {
        log::warn!("client {client_id} sent message after close or error");
    };

    let max_incoming_queue_length = state.config.incoming_queue_length.get();

    let (tx, rx) = mpsc::channel(max_incoming_queue_length);

    let mut tx = MeteredSender::with_gauge(tx, recv_queue_gauge.clone());
    let rx = MeteredReceiver::with_gauge(rx, recv_queue_gauge);
    let rx = MeteredReceiverStream { inner: rx };

    tokio::spawn(async move {
        while let Some(item) = ws.next().await {
            if let Err(e) = tx.try_send(item) {
                match e {
                    // If the queue is full, disconnect the client.
                    mpsc::error::TrySendError::Full(item) => {
                        let client_id = state.client_id;
                        log::warn!("Client {client_id} exceeded incoming_queue_length limit of {max_incoming_queue_length} requests");
                        // If we can't send close (send task already terminated):
                        //
                        // - Let downstream handlers know that we're closing,
                        //   so that remaining items in the queue are dropped.
                        //
                        // - Then exit the loop, as we won't be processing any
                        //   more messages, and we don't expect a close response
                        //   to arrive from the client.
                        if unordered_tx.send(CLOSE).is_err() {
                            state.close();
                            break;
                        }
                        // If we successfully enqueued `CLOSE`, enqueue `item`
                        // as well, as soon as there is space in the channel.
                        //
                        // This is to allow the client to complete the close
                        // handshake, for which the downstream handler needs to
                        // drain the queue.
                        //
                        // If `tx.send` fails, the pipeline is broken, so exit.
                        // See commentary on the `TrySendError::Closed` match
                        // arm below.
                        if tx.send(item).await.is_err() {
                            on_message_after_close(state.client_id);
                            break;
                        }
                    }
                    // If the downstream consumer went away,
                    // it has consumed a `Close` frame or `Err` value
                    // from the queue and thus has determined that it's done.
                    //
                    // Well-behaved clients shouldn't send anything after
                    // closing, so issue a warning.
                    //
                    // We're done either way, so break.
                    mpsc::error::TrySendError::Closed(_item) => {
                        on_message_after_close(state.client_id);
                        break;
                    }
                }
            }
        }
    });

    rx
}

/// Turns a [`MeteredReceiver`] into a [`Stream`],
/// like [`tokio_stream::wrappers::ReceiverStream`] does for [`mpsc::Receiver`].
struct MeteredReceiverStream<T> {
    inner: MeteredReceiver<T>,
}

impl<T> Stream for MeteredReceiverStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_recv(cx)
    }
}

/// Stream that consumes [`ClientMessage`]s and yields [`DataMessage`]s for
/// evaluation.
///
/// Calls `state.set_ponged()` if and when the input yields a pong message.
/// Calls `state.close()` if and when the input yields a close frame,
/// i.e. the client initiated a close handshake, which we track using the
/// `client_closed_metric`.
///
/// Terminates if and when the input stream terminates.
fn ws_client_message_handler(
    state: Arc<ActorState>,
    client_closed_metric: IntGauge,
    mut messages: impl Stream<Item = ClientMessage> + Unpin,
) -> impl Stream<Item = (DataMessage, Instant)> {
    stream! {
        while let Some(message) = messages.next().await {
            match message {
                ClientMessage::Message(message) => {
                    log::trace!("Received client message");
                    yield (message, Instant::now());
                },
                ClientMessage::Ping(_bytes) => {
                    log::trace!("Received ping from client {}", state.client_id);
                    // `tungstenite` will respond with `Pong` for us,
                    // no need to send it ourselves.
                },
                ClientMessage::Pong(_bytes) => {
                    log::trace!("Received pong from client {}", state.client_id);
                    state.set_ponged();
                },
                ClientMessage::Close(close_frame) => {
                    log::trace!("Received Close frame from client {}: {:?}", state.client_id, close_frame);
                    let was_closed = state.close();
                    // This is the client telling us they want to close.
                    if !was_closed {
                        client_closed_metric.inc();
                    }
                }
            }
        }
        log::trace!("client message handler done");
    }
}

/// Outgoing messages that don't need to be ordered wrt subscription updates.
#[derive(Debug, From)]
enum UnorderedWsMessage {
    /// Server-initiated close.
    Close(CloseFrame),
    /// Server-initiated ping.
    Ping(Bytes),
    /// Error calling a reducer.
    ///
    /// The error indicates that the reducer was **not** called,
    /// and can thus be unordered wrt subscription updates.
    Error(MessageExecutionError),
}

/// Abstraction over [`ClientConnectionReceiver`], so tests can use a plain
/// [`mpsc::Receiver`].
trait Receiver {
    fn recv(&mut self) -> impl Future<Output = Option<SerializableMessage>> + Send;
    fn close(&mut self);
}

impl Receiver for ClientConnectionReceiver {
    async fn recv(&mut self) -> Option<SerializableMessage> {
        ClientConnectionReceiver::recv(self).await
    }

    fn close(&mut self) {
        ClientConnectionReceiver::close(self);
    }
}

impl Receiver for mpsc::Receiver<SerializableMessage> {
    async fn recv(&mut self) -> Option<SerializableMessage> {
        mpsc::Receiver::recv(self).await
    }

    fn close(&mut self) {
        mpsc::Receiver::close(self);
    }
}

/// Sink that sends outgoing messages to the `ws` sink.
///
/// Consumes `messages`, which yields subscription updates and reducer call
/// results. Note that [`SerializableMessage`]s require serialization and
/// potentially compression, which can be costly.
/// Also consumes `unordered`, which yields [`UnorderedWsMessage`]s.
///
/// Terminates if:
///
/// - `unordered` is closed
/// - an error occurs sending to the `ws` sink
///
/// If an [`UnorderedWsMessage::Close`] is encountered, a close frame is sent
/// to the `ws` sink, and `state.close()` is called. When this happens,
/// `messages` will no longer be polled (no data can be sent after a close
/// frame anyways), so `messages.close()` will be called.
///
/// Keeps polling `unordered` if `state.closed()`, but discards all data.
/// This is so `ws_client_actor_inner` keeps polling the receive end of the
/// socket until the close handshake completes -- it would otherwise exit early
/// when sending to `unordered` fails.
async fn ws_send_loop(
    state: Arc<ActorState>,
    config: ClientConfig,
    mut ws: impl Sink<WsMessage, Error: Display> + Unpin,
    mut messages: impl Receiver,
    mut unordered: mpsc::UnboundedReceiver<UnorderedWsMessage>,
) {
    let mut serialize_buf = SerializeBuffer::new(config);

    loop {
        let closed = state.closed();

        tokio::select! {
            // `biased` towards the unordered queue,
            // which may initiate a connection shutdown.
            biased;

            maybe_msg = unordered.recv() => {
                let Some(msg) = maybe_msg else {
                    break;
                };
                // We shall not sent more data after a close frame,
                // but keep polling `unordered` so that `ws_client_actor` keeps
                // waiting for an acknowledgement from the client,
                // even if it spuriously initiates another close itself.
                if closed {
                    continue;
                }
                match msg {
                    UnorderedWsMessage::Close(close_frame) => {
                        log::trace!("sending close frame");
                        if let Err(e) = ws.send(WsMessage::Close(Some(close_frame))).await {
                            log::warn!("error sending close frame: {e:#}");
                            break;
                        }
                        // NOTE: It's ok to not update the state if we fail to
                        // send the close frame, because we assume that the main
                        // loop will exit when this future terminates.
                        state.close();
                        // We won't be polling `messages` anymore,
                        // so let senders know.
                        messages.close();
                    },
                    UnorderedWsMessage::Ping(bytes) => {
                        log::trace!("sending ping");
                        if let Err(e) = ws.feed(WsMessage::Ping(bytes)).await {
                            log::warn!("error sending ping: {e:#}");
                            break;
                        }
                    },
                    UnorderedWsMessage::Error(err) => {
                        log::trace!("sending error result");
                        let (msg_alloc, res) = send_message(
                            &state.database,
                            config,
                            serialize_buf,
                            None,
                            &mut ws,
                            err
                        ).await;
                        serialize_buf = msg_alloc;

                        if let Err(e) = res {
                            log::warn!("websocket send error: {e}");
                            break;
                        }
                    },
                }
            },

            maybe_message = messages.recv(), if !closed => {
                let Some(message) = maybe_message else {
                    // The message sender was dropped, even though no close
                    // handshake is in progress. This should not normally happen,
                    // but initiating close seems like the correct thing to do.
                    log::warn!("message sender dropped without close handshake");
                    if let Err(e) = ws.send(WsMessage::Close(None)).await {
                        log::warn!("error sending close frame: {e:#}");
                        break;
                    }
                    state.close();
                    // Continue so that `ws_client_actor` keeps waiting for an
                    // acknowledgement from the client.
                    continue;
                };
                log::trace!("sending outgoing message");
                let (msg_alloc, res) = send_message(
                    &state.database,
                    config,
                    serialize_buf,
                    message.workload().zip(message.num_rows()),
                    &mut ws,
                    message
                ).await;
                serialize_buf = msg_alloc;

                if let Err(e) = res {
                    log::warn!("websocket send error: {e}");
                    return;
                }
            },
        }

        if let Err(e) = ws.flush().await {
            log::warn!("error flushing websocket: {e}");
            break;
        }
    }
}

/// Serialize and potentially compress `message`, and feed it to the `ws` sink.
async fn send_message<S: Sink<WsMessage> + Unpin>(
    database_identity: &Identity,
    config: ClientConfig,
    serialize_buf: SerializeBuffer,
    metrics_metadata: Option<(WorkloadType, usize)>,
    ws: &mut S,
    message: impl ToProtocol<Encoded = SwitchedServerMessage> + Send + 'static,
) -> (SerializeBuffer, Result<(), S::Error>) {
    let (workload, num_rows) = metrics_metadata.unzip();
    // Move large messages to a rayon thread,
    // as serialization and compression can take a long time.
    // The threshold of 1024 rows is arbitrary, and may need to be refined.
    let serialize_and_compress = |serialize_buf, message, config| {
        let start = Instant::now();
        let (msg_alloc, msg_data) = serialize(serialize_buf, message, config);
        (start.elapsed(), msg_alloc, msg_data)
    };
    let (timing, msg_alloc, msg_data) = if num_rows.is_some_and(|n| n > 1024) {
        spawn_rayon(move || serialize_and_compress(serialize_buf, message, config)).await
    } else {
        serialize_and_compress(serialize_buf, message, config)
    };
    report_ws_sent_metrics(database_identity, workload, num_rows, timing, &msg_data);

    let res = async {
        ws.feed(datamsg_to_wsmsg(msg_data)).await?;
        // To reclaim the `msg_alloc` memory, we need `SplitSink` to push down
        // its item slot to the inner sink, which will copy the `Bytes` and
        // drop the reference.
        // We don't want to flush the inner sink just yet, as we might be
        // writing many messages.
        // `SplitSink::poll_ready` does what we want.
        poll_fn(|cx| ws.poll_ready_unpin(cx)).await
    }
    .await;
    // Reclaim can fail if we didn't succeed pushing down the data to the
    // websocket. We must return a buffer, though, so create a fresh one.
    let buf = msg_alloc.try_reclaim().unwrap_or_else(|| SerializeBuffer::new(config));

    (buf, res)
}

#[derive(Debug)]
enum ClientMessage {
    Message(DataMessage),
    Ping(Bytes),
    Pong(Bytes),
    Close(Option<CloseFrame>),
}

impl ClientMessage {
    fn from_message(msg: WsMessage) -> Self {
        match msg {
            WsMessage::Text(s) => Self::Message(DataMessage::Text(utf8bytes_to_bytestring(s))),
            WsMessage::Binary(b) => Self::Message(DataMessage::Binary(b)),
            WsMessage::Ping(b) => Self::Ping(b),
            WsMessage::Pong(b) => Self::Pong(b),
            WsMessage::Close(frame) => Self::Close(frame),
            // WebSocket::read_message() never returns a raw Message::Frame
            WsMessage::Frame(_) => unreachable!(),
        }
    }
}

/// Report metrics on sent rows and message sizes to a websocket client.
fn report_ws_sent_metrics(
    addr: &Identity,
    workload: Option<WorkloadType>,
    num_rows: Option<usize>,
    serialize_duration: Duration,
    msg_ws: &DataMessage,
) {
    // These metrics should be updated together,
    // or not at all.
    if let (Some(workload), Some(num_rows)) = (workload, num_rows) {
        WORKER_METRICS
            .websocket_sent_num_rows
            .with_label_values(addr, &workload)
            .observe(num_rows as f64);
        WORKER_METRICS
            .websocket_sent_msg_size
            .with_label_values(addr, &workload)
            .observe(msg_ws.len() as f64);
    }

    WORKER_METRICS
        .websocket_serialize_secs
        .with_label_values(addr)
        .observe(serialize_duration.as_secs_f64());
}

fn datamsg_to_wsmsg(msg: DataMessage) -> WsMessage {
    match msg {
        DataMessage::Text(text) => WsMessage::Text(bytestring_to_utf8bytes(text)),
        DataMessage::Binary(bin) => WsMessage::Binary(bin),
    }
}

fn utf8bytes_to_bytestring(s: Utf8Bytes) -> ByteString {
    // SAFETY: `Utf8Bytes` and `ByteString` have the same invariant of UTF-8 validity
    unsafe { ByteString::from_bytes_unchecked(Bytes::from(s)) }
}
fn bytestring_to_utf8bytes(s: ByteString) -> Utf8Bytes {
    // SAFETY: `Utf8Bytes` and `ByteString` have the same invariant of UTF-8 validity
    unsafe { Utf8Bytes::from_bytes_unchecked(s.into_bytes()) }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        pin::Pin,
        sync::atomic::AtomicUsize,
        task::{Context, Poll},
    };

    use anyhow::anyhow;
    use futures::{
        future::{self, Either, FutureExt as _},
        sink, stream,
    };
    use pretty_assertions::assert_matches;
    use spacetimedb::client::{messages::SerializableMessage, ClientName};
    use tokio::time::sleep;

    use super::*;

    fn dummy_client_id() -> ClientActorId {
        ClientActorId {
            identity: Identity::ZERO,
            connection_id: ConnectionId::ZERO,
            name: ClientName(0),
        }
    }

    fn dummy_actor_state() -> ActorState {
        dummy_actor_state_with_config(<_>::default())
    }

    fn dummy_actor_state_with_config(config: WebSocketOptions) -> ActorState {
        ActorState::new(Identity::ZERO, dummy_client_id(), config)
    }

    #[tokio::test]
    async fn idle_timer_extends_sleep() {
        let timeout = Duration::from_millis(10);

        let start = Instant::now();
        let (tx, rx) = watch::channel(start + timeout);
        tokio::join!(ws_idle_timer(rx), async {
            for _ in 0..5 {
                sleep(Duration::from_millis(1)).await;
                tx.send(Instant::now() + timeout).unwrap();
            }
        });
        let elapsed = start.elapsed();
        let expected = timeout + Duration::from_millis(5);
        assert!(
            elapsed >= expected,
            "{}ms elapsed, expected >= {}ms",
            elapsed.as_millis(),
            expected.as_millis(),
        );
    }

    #[tokio::test]
    async fn recv_loop_terminates_when_input_exhausted() {
        let state = Arc::new(dummy_actor_state());
        let (idle_tx, _idle_rx) = watch::channel(Instant::now() + state.config.idle_timeout);

        let input = stream::iter(vec![Ok(WsMessage::Ping(Bytes::new()))]);
        pin_mut!(input);

        let recv_loop = ws_recv_loop(state, idle_tx, input);
        pin_mut!(recv_loop);

        assert_matches!(recv_loop.next().await, Some(ClientMessage::Ping(_)));
        assert_matches!(recv_loop.next().await, None);
    }

    #[tokio::test]
    async fn recv_loop_terminates_when_input_yields_err() {
        let state = Arc::new(dummy_actor_state());
        let (idle_tx, _idle_rx) = watch::channel(Instant::now() + state.config.idle_timeout);

        let input = stream::iter(vec![
            Ok(WsMessage::Ping(Bytes::new())),
            Err(WsError::ConnectionClosed),
            Ok(WsMessage::Pong(Bytes::new())),
        ]);
        pin_mut!(input);

        let recv_loop = ws_recv_loop(state, idle_tx, input);
        pin_mut!(recv_loop);

        assert_matches!(recv_loop.next().await, Some(ClientMessage::Ping(_)));
        assert_matches!(recv_loop.next().await, None);
    }

    #[tokio::test]
    async fn recv_loop_drains_remaining_messages_when_closed() {
        let state = Arc::new(dummy_actor_state());
        let (idle_tx, _idle_rx) = watch::channel(Instant::now() + state.config.idle_timeout);

        let input = stream::iter(vec![
            Ok(WsMessage::Ping(Bytes::new())),
            Ok(WsMessage::Pong(Bytes::new())),
        ]);
        pin_mut!(input);
        {
            let recv_loop = ws_recv_loop(state.clone(), idle_tx, &mut input);
            pin_mut!(recv_loop);

            state.close();
            assert_matches!(recv_loop.next().await, None);
        }
        assert_matches!(input.next().await, None);
    }

    #[tokio::test]
    async fn recv_loop_stops_at_error_while_draining() {
        let state = Arc::new(dummy_actor_state());
        let (idle_tx, _idle_rx) = watch::channel(Instant::now() + state.config.idle_timeout);

        let input = stream::iter(vec![
            Ok(WsMessage::Ping(Bytes::new())),
            Err(WsError::ConnectionClosed),
            Ok(WsMessage::Pong(Bytes::new())),
        ]);
        pin_mut!(input);
        {
            let recv_loop = ws_recv_loop(state.clone(), idle_tx, &mut input);
            pin_mut!(recv_loop);

            state.close();
            assert_matches!(recv_loop.next().await, None);
        }
        assert_matches!(input.next().await, Some(Ok(WsMessage::Pong(_))));
    }

    #[tokio::test]
    async fn recv_loop_updates_idle_channel() {
        let state = Arc::new(dummy_actor_state());
        let idle_deadline = Instant::now() + state.config.idle_timeout;
        let (idle_tx, mut idle_rx) = watch::channel(idle_deadline);

        let input = stream::iter(vec![
            Ok(WsMessage::Ping(Bytes::new())),
            Ok(WsMessage::Pong(Bytes::new())),
        ]);
        let recv_loop = ws_recv_loop(state, idle_tx, input);
        pin_mut!(recv_loop);

        let mut new_idle_deadline = *idle_rx.borrow();
        while let Some(message) = recv_loop.next().await {
            drop(message);
            assert!(idle_rx.has_changed().unwrap());
            new_idle_deadline = *idle_rx.borrow_and_update();
        }
        assert!(new_idle_deadline > idle_deadline);
    }

    #[tokio::test]
    async fn client_message_handler_terminates_when_input_exhausted() {
        let state = Arc::new(dummy_actor_state());
        let metric = IntGauge::new("bleep", "unhelpful").unwrap();

        let input = stream::iter(vec![
            ClientMessage::Ping(Bytes::new()),
            ClientMessage::Message(DataMessage::from("hello".to_owned())),
        ]);
        let handler = ws_client_message_handler(state, metric, input);
        pin_mut!(handler);

        assert_matches!(
            handler.next().await,
            Some((DataMessage::Text(data), _instant)) if data == "hello"
        );
        assert_matches!(handler.next().await, None);
    }

    #[tokio::test]
    async fn client_message_handler_updates_pong_and_closed_states_and_metric() {
        let state = Arc::new(dummy_actor_state());
        state.reset_ponged();
        let metric = IntGauge::new("bleep", "unhelpful").unwrap();

        let input = stream::iter(vec![ClientMessage::Pong(Bytes::new()), ClientMessage::Close(None)]);
        let handler = ws_client_message_handler(state.clone(), metric.clone(), input);
        handler.map(drop).for_each(future::ready).await;

        assert!(state.closed());
        assert!(state.reset_ponged());
        assert_eq!(metric.get(), 1);
    }

    #[tokio::test]
    async fn send_loop_terminates_when_unordered_closed() {
        let state = Arc::new(dummy_actor_state());
        let (messages_tx, messages_rx) = mpsc::channel(64);
        let (unordered_tx, unordered_rx) = mpsc::unbounded_channel();

        let send_loop = ws_send_loop(
            state,
            ClientConfig::for_test(),
            sink::drain(),
            messages_rx,
            unordered_rx,
        );
        pin_mut!(send_loop);

        assert!(is_pending(&mut send_loop).await);
        drop(messages_tx);
        assert!(is_pending(&mut send_loop).await);

        drop(unordered_tx);
        send_loop.await;
    }

    #[tokio::test]
    async fn send_loop_close_message_closes_state_and_messages() {
        let state = Arc::new(dummy_actor_state());
        let (messages_tx, messages_rx) = mpsc::channel(64);
        let (unordered_tx, unordered_rx) = mpsc::unbounded_channel();

        let send_loop = ws_send_loop(
            state.clone(),
            ClientConfig::for_test(),
            sink::drain(),
            messages_rx,
            unordered_rx,
        );
        pin_mut!(send_loop);

        unordered_tx
            .send(UnorderedWsMessage::Close(CloseFrame {
                code: CloseCode::Away,
                reason: "done".into(),
            }))
            .unwrap();

        assert!(is_pending(&mut send_loop).await);
        assert!(state.closed());
        assert!(messages_tx.is_closed());
    }

    #[tokio::test]
    async fn send_loop_terminates_if_sink_cant_be_fed() {
        let input = [
            Either::Left(UnorderedWsMessage::Close(CloseFrame {
                code: CloseCode::Away,
                reason: "bah!".into(),
            })),
            Either::Left(UnorderedWsMessage::Ping(Bytes::new())),
            Either::Left(UnorderedWsMessage::Error(MessageExecutionError {
                reducer: None,
                reducer_id: None,
                caller_identity: Identity::ZERO,
                caller_connection_id: None,
                err: anyhow!("it did not work"),
            })),
            // TODO: This is the easiest to construct,
            // but maybe we want other variants, too.
            Either::Right(SerializableMessage::Identity(IdentityTokenMessage {
                identity: Identity::ZERO,
                token: "macaron".into(),
                connection_id: ConnectionId::ZERO,
            })),
        ];

        for message in input {
            let state = Arc::new(dummy_actor_state());
            let (messages_tx, messages_rx) = mpsc::channel(64);
            let (unordered_tx, unordered_rx) = mpsc::unbounded_channel();

            let send_loop = ws_send_loop(
                state.clone(),
                ClientConfig::for_test(),
                UnfeedableSink,
                messages_rx,
                unordered_rx,
            );
            pin_mut!(send_loop);

            match message {
                Either::Left(unordered) => unordered_tx.send(unordered).unwrap(),
                Either::Right(message) => messages_tx.send(message).await.unwrap(),
            }
            send_loop.await;
        }
    }

    #[tokio::test]
    async fn send_loop_terminates_if_sink_cant_be_flushed() {
        let input = [
            Either::Left(UnorderedWsMessage::Close(CloseFrame {
                code: CloseCode::Away,
                reason: "bah!".into(),
            })),
            Either::Left(UnorderedWsMessage::Ping(Bytes::new())),
            Either::Left(UnorderedWsMessage::Error(MessageExecutionError {
                reducer: None,
                reducer_id: None,
                caller_identity: Identity::ZERO,
                caller_connection_id: None,
                err: anyhow!("it did not work"),
            })),
            // TODO: This is the easiest to construct,
            // but maybe we want other variants, too.
            Either::Right(SerializableMessage::Identity(IdentityTokenMessage {
                identity: Identity::ZERO,
                token: "macaron".into(),
                connection_id: ConnectionId::ZERO,
            })),
        ];

        for message in input {
            let state = Arc::new(dummy_actor_state());
            let (messages_tx, messages_rx) = mpsc::channel(64);
            let (unordered_tx, unordered_rx) = mpsc::unbounded_channel();

            let send_loop = ws_send_loop(
                state.clone(),
                ClientConfig::for_test(),
                UnflushableSink,
                messages_rx,
                unordered_rx,
            );
            pin_mut!(send_loop);

            match message {
                Either::Left(unordered) => unordered_tx.send(unordered).unwrap(),
                Either::Right(message) => messages_tx.send(message).await.unwrap(),
            }
            send_loop.await;
        }
    }

    #[tokio::test]
    async fn main_loop_terminates_if_either_send_or_recv_terminates() {
        let state = Arc::new(dummy_actor_state());
        ws_main_loop(
            state.clone(),
            future::pending,
            future::pending(),
            tokio::spawn(sleep(Duration::from_millis(10))),
            tokio::spawn(future::pending()),
            drop,
        )
        .await;
        ws_main_loop(
            state,
            future::pending,
            future::pending(),
            tokio::spawn(future::pending()),
            tokio::spawn(sleep(Duration::from_millis(10))),
            drop,
        )
        .await;
    }

    #[tokio::test]
    async fn main_loop_terminates_on_idle_timeout() {
        let state = Arc::new(dummy_actor_state_with_config(WebSocketOptions {
            idle_timeout: Duration::from_millis(10),
            ..<_>::default()
        }));
        let (idle_tx, idle_rx) = watch::channel(state.next_idle_deadline());

        let start = Instant::now();
        let mut t = tokio::spawn({
            let state = state.clone();
            async move {
                ws_main_loop(
                    state,
                    future::pending,
                    ws_idle_timer(idle_rx),
                    tokio::spawn(future::pending()),
                    tokio::spawn(future::pending()),
                    drop,
                )
                .await
            }
        });

        let loop_start = Instant::now();
        for _ in 0..5 {
            sleep(Duration::from_millis(5)).await;
            idle_tx.send(state.next_idle_deadline()).unwrap();
            assert!(is_pending(&mut t).await);
        }
        let timeout = loop_start.elapsed() + Duration::from_millis(10);

        t.await.unwrap();
        let elapsed = start.elapsed();
        assert!(elapsed >= timeout);
        assert!(elapsed < timeout + Duration::from_millis(10));
    }

    #[tokio::test]
    async fn main_loop_keepalive_keeps_alive() {
        let state = Arc::new(dummy_actor_state_with_config(WebSocketOptions {
            ping_interval: Duration::from_millis(5),
            idle_timeout: Duration::from_millis(10),
            ..<_>::default()
        }));
        let (idle_tx, idle_rx) = watch::channel(state.next_idle_deadline());
        // Pretend we received a pong immediately after sending a ping,
        // but only five times.
        let unordered_tx = {
            let state = state.clone();
            let pings = AtomicUsize::new(0);
            move |m| {
                if let UnorderedWsMessage::Ping(_) = m {
                    let n = pings.fetch_add(1, Ordering::Relaxed);
                    if n < 5 {
                        state.set_ponged();
                        idle_tx.send(state.next_idle_deadline()).ok();
                    }
                }
            }
        };

        let start = Instant::now();
        let t = tokio::spawn({
            let state = state.clone();
            async move {
                ws_main_loop(
                    state,
                    future::pending,
                    ws_idle_timer(idle_rx),
                    tokio::spawn(future::pending()),
                    tokio::spawn(future::pending()),
                    unordered_tx,
                )
                .await
            }
        });

        let expected_timeout = (5 * state.config.ping_interval) + state.config.idle_timeout;
        let res = timeout(expected_timeout, t).await;
        let elapsed = start.elapsed();

        // It didn't time out.
        assert_matches!(res, Ok(Ok(())));
        // It didn't exit early. Allow it to miss a ping.
        assert!(elapsed >= expected_timeout - state.config.ping_interval);
    }

    #[tokio::test]
    async fn main_loop_terminates_when_module_exits() {
        let state = Arc::new(dummy_actor_state());

        let (_idle_tx, idle_rx) = watch::channel(state.next_idle_deadline());
        let unordered_tx = {
            let state = state.clone();
            move |m| {
                if let UnorderedWsMessage::Close(_) = m {
                    state.close();
                }
            }
        };

        let start = Instant::now();
        tokio::spawn(async move {
            let hotswap = || async {
                sleep(Duration::from_millis(5)).await;
                Err(NoSuchModule)
            };

            ws_main_loop(
                state.clone(),
                hotswap,
                ws_idle_timer(idle_rx),
                // Pretend we received a close immediately after sending one.
                tokio::spawn(async move {
                    loop {
                        if state.closed() {
                            break;
                        }
                        sleep(Duration::from_millis(1)).await
                    }
                }),
                tokio::spawn(future::pending()),
                unordered_tx,
            )
            .await
        })
        .await
        .unwrap();
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(5));
        assert!(elapsed < Duration::from_millis(10));
    }

    #[tokio::test]
    async fn recv_queue_sends_close_when_at_capacity() {
        let state = Arc::new(dummy_actor_state_with_config(WebSocketOptions {
            incoming_queue_length: 10.try_into().unwrap(),
            ..<_>::default()
        }));

        let (unordered_tx, mut unordered_rx) = mpsc::unbounded_channel();
        let input = stream::iter((0..20).map(|i| Ok(WsMessage::text(format!("message {i}")))));

        let metric = IntGauge::new("bleep", "unhelpful").unwrap();
        let received = ws_recv_queue(state, unordered_tx, metric.clone(), input)
            .collect::<Vec<_>>()
            .await;

        assert_matches!(unordered_rx.recv().await, Some(UnorderedWsMessage::Close(_)));
        // Queue length metric should be zero
        assert_eq!(metric.get(), 0);
        // Should have received all of the input.
        assert_eq!(received.len(), 20);
    }

    #[tokio::test]
    async fn recv_queue_closes_state_if_sender_gone() {
        let state = Arc::new(dummy_actor_state_with_config(WebSocketOptions {
            incoming_queue_length: 10.try_into().unwrap(),
            ..<_>::default()
        }));

        let (unordered_tx, _) = mpsc::unbounded_channel();
        let input = stream::iter((0..20).map(|i| Ok(WsMessage::text(format!("message {i}")))));

        let metric = IntGauge::new("bleep", "unhelpful").unwrap();
        let received = ws_recv_queue(state.clone(), unordered_tx, metric.clone(), input)
            .collect::<Vec<_>>()
            .await;

        assert!(state.closed());
        // Queue length metric should be zero
        assert_eq!(metric.get(), 0);
        // Should have received up to capacity.
        assert_eq!(received.len(), 10);
    }

    async fn is_pending(fut: &mut (impl Future + Unpin)) -> bool {
        poll_fn(|cx| Poll::Ready(fut.poll_unpin(cx).is_pending())).await
    }

    struct UnfeedableSink;

    impl<T> Sink<T> for UnfeedableSink {
        type Error = &'static str;

        fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(self: Pin<&mut Self>, _: T) -> Result<(), Self::Error> {
            Err("don't feed the sink")
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    struct UnflushableSink;

    impl<T> Sink<T> for UnflushableSink {
        type Error = &'static str;

        fn poll_ready(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(self: Pin<&mut Self>, _: T) -> Result<(), Self::Error> {
            Ok(())
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Err("don't flush the sink"))
        }

        fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn options_toml_roundtrip() {
        let options = WebSocketOptions::default();
        let toml = toml::to_string(&options).unwrap();
        assert_eq!(options, toml::from_str::<WebSocketOptions>(&toml).unwrap());
    }

    #[test]
    fn options_from_partial_toml() {
        let toml = r#"
            ping-interval = "53s"
            idle-timeout = "1m 3s"
"#;

        let expected = WebSocketOptions {
            ping_interval: Duration::from_secs(53),
            idle_timeout: Duration::from_secs(63),
            ..<_>::default()
        };

        assert_eq!(expected, toml::from_str(toml).unwrap());
    }
}
