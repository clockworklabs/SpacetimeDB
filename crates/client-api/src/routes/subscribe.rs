use std::future::poll_fn;
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Extension;
use axum_extra::TypedHeader;
use bytes::Bytes;
use bytestring::ByteString;
use derive_more::From;
use futures::{pin_mut, FutureExt, Sink, SinkExt, Stream, StreamExt};
use http::{HeaderValue, StatusCode};
use prometheus::IntGauge;
use scopeguard::ScopeGuard;
use serde::Deserialize;
use spacetimedb::client::messages::{
    serialize, IdentityTokenMessage, SerializableMessage, SerializeBuffer, SwitchedServerMessage, ToProtocol,
};
use spacetimedb::client::{
    ClientActorId, ClientConfig, ClientConnection, DataMessage, MessageExecutionError, MessageHandleError,
    MeteredReceiver, Protocol,
};
use spacetimedb::execution_context::WorkloadType;
use spacetimedb::host::module_host::ClientConnectedError;
use spacetimedb::host::NoSuchModule;
use spacetimedb::util::spawn_rayon;
use spacetimedb::worker_metrics::WORKER_METRICS;
use spacetimedb::Identity;
use spacetimedb_client_api_messages::websocket::{self as ws_api, Compression};
use spacetimedb_lib::connection_id::{ConnectionId, ConnectionIdForUrl};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::tungstenite::Utf8Bytes;

use crate::auth::SpacetimeAuth;
use crate::util::websocket::{
    CloseCode, CloseFrame, Message as WsMessage, WebSocketConfig, WebSocketStream, WebSocketUpgrade, WsError,
};
use crate::util::{NameOrIdentity, XForwardedFor};
use crate::{log_and_500, ControlStateDelegate, NodeDelegate};

#[allow(clippy::declare_interior_mutable_const)]
pub const TEXT_PROTOCOL: HeaderValue = HeaderValue::from_static(ws_api::TEXT_PROTOCOL);
#[allow(clippy::declare_interior_mutable_const)]
pub const BIN_PROTOCOL: HeaderValue = HeaderValue::from_static(ws_api::BIN_PROTOCOL);

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
    }): Query<SubscribeQueryParams>,
    forwarded_for: Option<TypedHeader<XForwardedFor>>,
    Extension(auth): Extension<SpacetimeAuth>,
    ws: WebSocketUpgrade,
) -> axum::response::Result<impl IntoResponse>
where
    S: NodeDelegate + ControlStateDelegate,
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

    let (res, ws_upgrade, protocol) =
        ws.select_protocol([(BIN_PROTOCOL, Protocol::Binary), (TEXT_PROTOCOL, Protocol::Text)]);

    let protocol = protocol.ok_or((StatusCode::BAD_REQUEST, "no valid protocol selected"))?;
    let client_config = ClientConfig {
        protocol,
        compression,
        tx_update_full: !light,
    };

    // TODO: Should also maybe refactor the code and the protocol to allow a single websocket
    // to connect to multiple modules

    let database = ctx
        .get_database_by_identity(&db_identity)
        .unwrap()
        .ok_or(StatusCode::NOT_FOUND)?;

    let leader = ctx
        .leader(database.id)
        .await
        .map_err(log_and_500)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let identity_token = auth.creds.token().into();

    let module_rx = leader.module_watcher().await.map_err(log_and_500)?;

    let client_id = ClientActorId {
        identity: auth.identity,
        connection_id,
        name: ctx.client_actor_index().next_client_name(),
    };

    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(0x2000000))
        .max_frame_size(None)
        .accept_unmasked_frames(false);

    tokio::spawn(async move {
        let ws = match ws_upgrade.upgrade(ws_config).await {
            Ok(ws) => ws,
            Err(err) => {
                log::error!("WebSocket init error: {}", err);
                return;
            }
        };

        match forwarded_for {
            Some(TypedHeader(XForwardedFor(ip))) => {
                log::debug!("New client connected from ip {}", ip)
            }
            None => log::debug!("New client connected from unknown ip"),
        }

        let actor = |client, sendrx| ws_client_actor(client, ws, sendrx);
        let client = match ClientConnection::spawn(client_id, client_config, leader.replica_id, module_rx, actor).await
        {
            Ok(s) => s,
            Err(e @ (ClientConnectedError::Rejected(_) | ClientConnectedError::OutOfEnergy)) => {
                log::info!("{e}");
                return;
            }
            Err(e @ (ClientConnectedError::DBError(_) | ClientConnectedError::ReducerCall(_))) => {
                log::warn!("ModuleHost died while we were connecting: {e:#}");
                return;
            }
        };

        // Send the client their identity token message as the first message
        // NOTE: We're adding this to the protocol because some client libraries are
        // unable to access the http response headers.
        // Clients that receive the token from the response headers should ignore this
        // message.
        let message = IdentityTokenMessage {
            identity: auth.identity,
            token: identity_token,
            connection_id,
        };
        if let Err(e) = client.send_message(message) {
            log::warn!("{e}, before identity token was sent")
        }
    });

    Ok(res)
}

const LIVELINESS_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct ActorState {
    pub client_id: ClientActorId,
    pub database: Identity,
    closed: Arc<AtomicBool>,
    got_pong: Arc<AtomicBool>,
}

impl ActorState {
    fn new(database: Identity, client_id: ClientActorId) -> Self {
        Self {
            database,
            client_id,
            closed: Arc::new(AtomicBool::new(false)),
            got_pong: Arc::new(AtomicBool::new(true)),
        }
    }

    fn closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    fn close(&self) -> bool {
        self.closed.swap(true, Ordering::Relaxed)
    }

    fn set_ponged(&self) {
        self.got_pong.store(true, Ordering::Relaxed);
    }

    fn reset_ponged(&self) -> bool {
        self.got_pong.swap(false, Ordering::Relaxed)
    }
}

async fn ws_client_actor(client: ClientConnection, ws: WebSocketStream, sendrx: MeteredReceiver<SerializableMessage>) {
    // ensure that even if this task gets cancelled, we always cleanup the connection
    let mut client = scopeguard::guard(client, |client| {
        tokio::spawn(client.disconnect());
    });

    ws_client_actor_inner(&mut client, ws, sendrx).await;

    ScopeGuard::into_inner(client).disconnect().await;
}

async fn ws_client_actor_inner(
    client: &mut ClientConnection,
    ws: WebSocketStream,
    sendrx: MeteredReceiver<SerializableMessage>,
) {
    let database = client.module.info().database_identity;

    let client_closed_metric = WORKER_METRICS.ws_clients_closed_connection.with_label_values(&database);
    let incoming_queue_length = WORKER_METRICS.total_incoming_queue_length.with_label_values(&database);

    let state = ActorState::new(database, client.id);

    let mut liveness_check_interval = tokio::time::interval(LIVELINESS_TIMEOUT);

    // Channel for [`UnorderedWsMessage`]s.
    let (unordered_tx, unordered_rx) = mpsc::unbounded_channel();
    // Channel for submitting work to the [`ws_eval_handler`].
    //
    // Note that we buffer client messages unboundedly, so that we don't delay
    // subscription updates in the `select!` loop while we're waiting for an
    // evaluation result.
    // Being able to observe the backlog (via `incoming_queue_length`) is useful
    // to identify performance issues.
    // Yet, we may consider to instead spawn a task for the receive end, and not
    // buffer at all, in order to apply backpressure to client.
    let (eval_tx, eval_rx) = mpsc::unbounded_channel();
    let mut eval_rx = scopeguard::guard(UnboundedReceiverStream::new(eval_rx), |stream| {
        incoming_queue_length.sub(stream.into_inner().len() as _);
    });

    // Split websocket into send and receive halves.
    let (ws_send, ws_recv) = ws.split();
    // Make a stream that reads from the socket and yields [`ClientMessage`]s.
    let recv_loop = pin!(ws_recv_loop(state.clone(), ws_recv));
    let recv_handler = ws_client_message_handler(state.clone(), client_closed_metric, recv_loop);
    // Stream that consumes from `eval_tx` and evaluates the tasks.
    // Yields `Result<(), MessageHandleError>`.
    let eval_handler = ws_eval_handler(client.clone(), &mut *eval_rx);
    // Sink that sends subscription updates and reducer results from `sendrx`,
    // as well as [`UnorderedWsMessage`]s to the socket..
    let send_loop = ws_send_loop(state.clone(), client.config, ws_send, sendrx, unordered_rx);
    tokio::spawn(send_loop);

    pin_mut!(recv_handler);
    pin_mut!(eval_handler);

    loop {
        tokio::select! {
            // Get the next client message and submit it for evaluation.
            res = recv_handler.next() => {
                match res {
                    Some(task) => {
                        log::trace!("received new task");
                        if eval_tx.send(task).is_err() {
                            log::trace!("eval_tx already closed");
                            break;
                        };
                        incoming_queue_length.inc();
                    },
                    None => {
                        log::trace!("recv handler exhausted");
                        break;
                    }
                }
            },
            // Get the next evaluation result and handle errors.
            Some(result) = eval_handler.next() => {
                log::trace!("received task result");
                incoming_queue_length.dec();
                if let Err(e) = result {
                    if let MessageHandleError::Execution(err) = e {
                        log::error!("{err:#}");
                        let _ = unordered_tx.send(err.into());
                        continue;
                    }
                    log::debug!("Client caused error: {e}");
                    let close = CloseFrame {
                        code: CloseCode::Error,
                        reason: format!("{e:#}").into()
                    };
                    // If the sender is already gone,
                    // we won't be sending close, and not receive an ack,
                    // so exit the loop here.
                    if unordered_tx.send(close.into()).is_err() {
                        log::trace!("unordered_tx already closed");
                        break;
                    }
                }
            },

            // Update the client's module host if it was hotswapped,
            // or close the session if the module exited.
            //
            // Branch is disabled if we already sent a close frame.
            res = client.watch_module_host(), if !state.closed() => {
                if let Err(NoSuchModule) = res {
                    let close = CloseFrame {
                        code: CloseCode::Away,
                        reason: "module exited".into()
                    };
                    // If the sender is already gone,
                    // we won't be sending close, and not receive an ack,
                    // so exit the loop here.
                    if unordered_tx.send(close.into()).is_err() {
                        log::trace!("unordered_tx already closed");
                        break;
                    };
                }
            },

            // Send ping or time out the client.
            //
            // Branch is disabled if we lready sent a close frame.
            _ = liveness_check_interval.tick(), if !state.closed() => {
                let was_ponged = state.reset_ponged();
                if was_ponged {
                    // If the sender is already gone,
                    // we expect to receive an error on the receiver stream,
                    // but we can just as well exit here.
                    if unordered_tx.send(UnorderedWsMessage::Ping(Bytes::new())).is_err() {
                        log::trace!("unordered_tx already closed");
                        break;
                    };
                } else {
                    log::warn!("client {} timed out", client.id);
                    break;
                }
            }

            else => break,
        }
    }
    log::info!("Client connection ended: {}", client.id);
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
    state: ActorState,
    mut ws: impl Stream<Item = Result<WsMessage, WsError>> + Unpin,
) -> impl Stream<Item = ClientMessage> {
    stream! {
        loop {
            let Some(res) = async {
                if state.closed() {
                    log::trace!("await next client message with timeout");
                    match timeout(Duration::from_millis(150), ws.next()).await {
                        Err(_) => {
                            log::warn!("timeout waiting for client close");
                            None
                        },
                        Ok(item) => item
                    }
                } else {
                    log::trace!("await next client message without timeout");
                    ws.next().await
                }
            }.await else {
                log::trace!("recv stream exhausted");
                break;
            };
            match res {
                Ok(m) => {
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
                    | WsError::Utf8
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

/// Stream that consumes [`ClientMessage`]s and yields [`DataMessage`]s for
/// evaluation.
///
/// Calls `state.set_ponged()` if and when the input yields a pong message.
/// Calls `state.close()` if and when the input yields a close frame,
/// i.e. the client initiated a close handshake, which we track using the
/// `client_closed_metric`.
///
/// Terminates when the input stream terminates.
fn ws_client_message_handler(
    state: ActorState,
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

/// Stream that consumed [`DataMessage`]s, evaluates them, and yields the result.
///
/// Terminates when the input stream terminates.
fn ws_eval_handler(
    client: ClientConnection,
    mut messages: impl Stream<Item = (DataMessage, Instant)> + Unpin,
) -> impl Stream<Item = Result<(), MessageHandleError>> {
    stream! {
        while let Some((message, timer)) = messages.next().await {
            let result = client.handle_message(message, timer).await;
            yield result;
        }
    }
}

/// Outgoing messages that don't need to be ordered wrt subscription updates.
#[derive(From)]
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
    state: ActorState,
    config: ClientConfig,
    mut ws: impl Sink<WsMessage, Error = WsError> + Unpin,
    mut messages: MeteredReceiver<SerializableMessage>,
    mut unordered: mpsc::UnboundedReceiver<UnorderedWsMessage>,
) {
    let mut messages_buf = Vec::with_capacity(32);
    let mut serialize_buf = SerializeBuffer::new(config);

    loop {
        tokio::select! {
            // `biased` towards the unordered queue,
            // which may initiate a connection shutdown.
            biased;

            Some(msg) = unordered.recv() => {
                // We shall not sent more data after a close frame,
                // but keep polling `unordered` so that `ws_client_actor` keeps
                // waiting for an acknowledgement from the client,
                // even if it spuriously initiates another close itself.
                if state.closed() {
                    continue;
                }
                match msg {
                    UnorderedWsMessage::Close(close_frame) => {
                        log::trace!("sending close frame");
                        if let Err(e) = ws.send(WsMessage::Close(Some(close_frame))).await {
                            log::warn!("error sending close frame: {e:#}");
                            break;
                        }
                        state.close();
                        // We won't be polling `messages` anymore,
                        // so let senders know.
                        messages.close();
                    },
                    UnorderedWsMessage::Ping(bytes) => {
                        log::trace!("sending ping");
                        let _ = ws
                            .feed(WsMessage::Ping(bytes))
                            .await
                            .inspect_err(|e| log::warn!("error sending ping: {e:#}"));
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

            Some(n) = messages.recv_many(&mut messages_buf, 32).map(|n| (n != 0).then_some(n)), if !state.closed() => {
                log::trace!("sending {n} outgoing messages");
                for msg in messages_buf.drain(..n) {
                    let (msg_alloc, res) = send_message(
                        &state.database,
                        config,
                        serialize_buf,
                        msg.workload().zip(msg.num_rows()),
                        &mut ws,
                        msg
                    ).await;
                    serialize_buf = msg_alloc;

                    if let Err(e) = res {
                        log::warn!("websocket send error: {e}");
                        messages.close();
                        break;
                    }
                }
            },

            else => break,
        }

        if let Err(e) = ws.flush().await {
            log::warn!("error flushing websocket: {e}");
            break;
        }
    }
}

/// Serialize and potentially compress `message`, and feed it to the `ws` sink.
async fn send_message(
    database_identity: &Identity,
    config: ClientConfig,
    serialize_buf: SerializeBuffer,
    metrics_metadata: Option<(WorkloadType, usize)>,
    ws: &mut (impl Sink<WsMessage, Error = WsError> + Unpin),
    message: impl ToProtocol<Encoded = SwitchedServerMessage> + Send + 'static,
) -> (SerializeBuffer, Result<(), WsError>) {
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
