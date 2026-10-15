use crate::auth::idc::IdcTokenSigner;
use crate::db::relational_db::{RelationalDB, Tx};
use crate::host::idc::{DeliveryStreams, StreamId};
use crate::host::module_host::WeakModuleHost;
use crate::host::module_host::{CallReducerParams, ModuleInfo};
use crate::host::wasm_common::module_host_actor::{InstanceCommon, WasmInstance};
use crate::host::{FunctionArgs, ReducerOutcome};
use anyhow::Context;
use bytes::Bytes;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use spacetimedb_client_api_messages::idc::IdcResponse;
use spacetimedb_datastore::execution_context::{ExecutionContext, ReducerContext, Workload};
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
use spacetimedb_datastore::system_tables::{StInboundMsgResultStatus, StOutboundMsgRow, StOutboundStreamRow};
use spacetimedb_datastore::traits::IsolationLevel;
use spacetimedb_lib::{Hash, Identity, ProductValue, Timestamp};
use spacetimedb_primitives::{ColId, TableId};
use spacetimedb_schema::reducer_name::ReducerName;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const NOTIFICATION_BATCH_SIZE: usize = 16;

pub type IdcActorSender = mpsc::UnboundedSender<StOutboundMsgRow>;

/// HTTP and signing settings shared by the host's IDC actors.
#[derive(Clone)]
pub struct IdcConfig {
    /// Local HTTP port used to send requests.
    pub http_port: u16,
    /// Signs the sender's database identity. These tokens are not exposed to module code.
    pub token_signer: Arc<dyn IdcTokenSigner>,
}

pub struct IdcActor {
    _abort: tokio::task::AbortHandle,
}

impl Drop for IdcActor {
    fn drop(&mut self) {
        self._abort.abort();
    }
}

pub struct IdcActorStarter {
    rx: mpsc::UnboundedReceiver<StOutboundMsgRow>,
}

impl IdcActorStarter {
    pub fn start(
        self,
        db: Arc<RelationalDB>,
        database_identity: Identity,
        config: IdcConfig,
        module_host: WeakModuleHost,
    ) -> anyhow::Result<IdcActor> {
        let mut streams = DeliveryStreams::default();
        restore_pending_streams(&db, &mut streams)?;
        let abort = tokio::spawn(async move {
            if let Err(error) = run_idc_loop(db, database_identity, config, module_host, self.rx, streams).await {
                log::error!(
                    "idc_actor: sender_identity={}: stopped: {error:#}",
                    database_identity.to_hex(),
                );
            }
        })
        .abort_handle();
        Ok(IdcActor { _abort: abort })
    }
}

impl IdcActor {
    pub fn open() -> (IdcActorStarter, IdcActorSender) {
        let (tx, rx) = mpsc::unbounded_channel();
        (IdcActorStarter { rx }, tx)
    }
}

/// An outbox message and its metadata from the `st_outbound_*` tables.
#[derive(Clone)]
struct PendingMessage {
    stream_id: StreamId,
    /// Primary key of the user outbox row.
    message_id: u64,
    sequence: u64,
    outbox_table_id: TableId,
    target_identity: Identity,
    /// All sequences through this value are complete. The receiver can discard their results.
    ack_prefix: u64,
    target_reducer: ReducerName,
    signature_hash: Hash,
    /// BSATN argument product in receiver parameter order, without field names or a field count.
    reducer_args: Bytes,
    /// Full row used for delivery and passed to `on_result`.
    outbox_row: ProductValue,
    on_result_reducer: Option<ReducerName>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MessageResult {
    status: StInboundMsgResultStatus,
    /// Raw HTTP response body. Errors are UTF-8 text, not BSATN strings.
    /// Success bodies are empty for new calls. Retries may return saved BSATN return bytes.
    /// These bytes are stored but are not decoded for `on_result`.
    payload: Bytes,
}

/// A message whose receiver result has been stored locally.
#[derive(Clone)]
pub(super) struct DeliveredMessage {
    message: PendingMessage,
    result: MessageResult,
}

enum LoadedMessage {
    /// Needs delivery to the receiver.
    Pending(PendingMessage),
    /// Has a stored result and is ready for callback and cleanup.
    Delivered(DeliveredMessage),
}

impl LoadedMessage {
    fn message(&self) -> &PendingMessage {
        match self {
            Self::Pending(message) => message,
            Self::Delivered(delivered) => &delivered.message,
        }
    }
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        // The local proxy handles routing. A redirect is not an IDC result.
        .redirect(reqwest::redirect::Policy::none())
        .http2_prior_knowledge()
        .http2_keep_alive_interval(Some(Duration::from_millis(200)))
        .http2_keep_alive_timeout(Duration::from_secs(5))
        .http2_keep_alive_while_idle(true)
        .timeout(Duration::from_secs(5))
        .build()
        .expect("failed to build IDC reqwest client")
}

/// Process one message at a time per stream. Other streams can run at the same time.
///
/// For each message:
/// 1. Send it using its stored stream ID and sequence. If the reply is lost, retry
///    with those same IDs. The receiver returns the saved result instead of
///    executing the reducer again.
/// 2. Save the reply locally before calling `on_result`. If a retry or restart
///    finds a saved reply, skip the HTTP request and use that reply.
/// 3. Run `on_result`, if configured and the outbox row still matches. Commit its
///    changes, delete the message, and advance `ack_prefix` in one transaction.
///    If the callback fails, roll back its changes and clean up separately.
/// 4. Move to the next message after cleanup commits. If no result was received,
///    wait and retry the same sequence. A reducer or callback failure does not
///    require sending the message again.
///
/// Load pending streams at startup. After that, new inserts, completed attempts,
/// and retry timers wake the loop. Read each stream's next message from the database.
/// Send acknowledgments with later messages. Idle streams do not send a separate ack.
/// If loading a message fails, stop the actor instead of retrying the read.
async fn run_idc_loop(
    db: Arc<RelationalDB>,
    database_identity: Identity,
    config: IdcConfig,
    module_host: WeakModuleHost,
    mut notify_rx: mpsc::UnboundedReceiver<StOutboundMsgRow>,
    mut streams: DeliveryStreams,
) -> anyhow::Result<()> {
    let client = http_client();
    let mut in_flight = FuturesUnordered::new();
    let mut notifications = Vec::with_capacity(NOTIFICATION_BATCH_SIZE);
    let mut notifications_open = true;

    loop {
        // Start ready messages. Each future includes delivery, callback, and cleanup.
        // Stopping the actor drops these futures, but cannot cancel work already
        // running on the receiver or a callback already submitted to the local host.
        while let Some(msg) = take_ready_message(&db, &mut streams)? {
            let stream_id = msg.message().stream_id;
            let seq = msg.message().sequence;
            let (db, client, config, module_host) = (&db, &client, &config, &module_host);
            in_flight.push(async move {
                let result = handle_message(db, client, database_identity, config, module_host, msg).await;
                (stream_id, seq, result)
            });
        }

        let next_retry = streams.next_retry();

        tokio::select! {
            // Start the next message after cleanup succeeds. Retry failed attempts after backoff.
            Some((stream_id, seq, result)) = in_flight.next(), if !in_flight.is_empty() => {
                match result {
                    Ok(()) => {
                        streams.finish(stream_id, seq);
                    }
                    Err(error) => {
                        let delay = streams.retry(stream_id, seq);
                        log::warn!(
                            "idc_actor: sender_identity={} stream_id={stream_id} sequence={seq}: retrying after {delay:?}: {error:#}",
                            database_identity.to_hex(),
                        );
                    }
                }
            },

            // New inserts wake ready streams, but do not clear backoff.
            // Bound each batch so a steady flow of inserts cannot hold up completions or retries.
            count = notify_rx.recv_many(&mut notifications, NOTIFICATION_BATCH_SIZE),
                if notifications_open =>
            {
                // Zero means the channel is closed and drained.
                notifications_open = count != 0;
                for msg in notifications.drain(..) {
                    streams.push(StreamId(msg.stream_id));
                }
            },

            // Wake the stream with the earliest retry deadline.
            Some(stream_id) = async {
                let (stream_id, delay) = next_retry?;
                tokio::time::sleep(delay).await;
                Some(stream_id)
            }, if next_retry.is_some() => {
                streams.push(stream_id);
            },

            // The notification channel is drained and no deliveries or retries remain.
            else => {
                log::debug!(
                    "idc_actor: sender_identity={}: stopping; notification channel closed and no deliveries or retries remain",
                    database_identity.to_hex(),
                );
                break;
            },
        };
    }
    Ok(())
}

/// Take the next ready message and mark its stream as active until processing finishes.
fn take_ready_message(db: &RelationalDB, streams: &mut DeliveryStreams) -> anyhow::Result<Option<LoadedMessage>> {
    while let Some(stream_id) = streams.pop_ready() {
        match read_next_ordered_message(db, stream_id)
            .with_context(|| format!("failed to read next message for stream {stream_id}"))?
        {
            Some(msg) => {
                streams.start(stream_id, msg.message().sequence);
                return Ok(Some(msg));
            }
            None => {
                // There is no work left. A later insert adds the stream back.
                streams.remove_idle(stream_id);
            }
        }
    }
    Ok(None)
}

/// Keep the stream active through delivery, callback, and cleanup.
/// Errors leave the message pending for retry. A saved result skips the HTTP request.
async fn handle_message(
    db: &RelationalDB,
    client: &reqwest::Client,
    database_identity: Identity,
    config: &IdcConfig,
    module_host: &WeakModuleHost,
    msg: LoadedMessage,
) -> anyhow::Result<()> {
    let msg = match msg {
        LoadedMessage::Pending(msg) => {
            let Some(delivered) = deliver_message_and_record_result(db, client, database_identity, config, msg).await?
            else {
                return Ok(());
            };
            delivered
        }
        LoadedMessage::Delivered(msg) => msg,
    };
    finalize_message(module_host, msg).await
}

/// Send the message and save its result before running the callback.
/// Saving the result does not advance `ack_prefix`. That happens during callback and cleanup.
/// A saved result lets us resume after a restart without sending the request again.
/// Returns `None` for a late reply to a message we already cleaned up and acknowledged.
async fn deliver_message_and_record_result(
    db: &RelationalDB,
    client: &reqwest::Client,
    database_identity: Identity,
    config: &IdcConfig,
    msg: PendingMessage,
) -> anyhow::Result<Option<DeliveredMessage>> {
    let response = attempt_delivery(client, database_identity, config, &msg).await;
    match response.and_then(|response| record_delivery_response(db, &msg, response)) {
        Ok(result) => Ok(result.map(|result| DeliveredMessage { message: msg, result })),
        Err(error) => {
            log::warn!(
                "idc_actor: sender_identity={} stream_id={} sequence={} target_identity={} outbox_table_id={:?}: delivery failed: {error:#}",
                database_identity.to_hex(),
                msg.stream_id,
                msg.sequence,
                msg.target_identity.to_hex(),
                msg.outbox_table_id,
            );
            record_transport_error(db, &msg, format!("{error:#}"));
            Err(error)
        }
    }
}

/// Save the receiver's result. Ignore `AlreadyAcked` only if local cleanup already finished.
fn record_delivery_response(
    db: &RelationalDB,
    msg: &PendingMessage,
    response: IdcResponse,
) -> anyhow::Result<Option<MessageResult>> {
    let (status, payload) = match response {
        IdcResponse::Success(payload) => (StInboundMsgResultStatus::Ok, payload),
        IdcResponse::TerminalError(payload) => (StInboundMsgResultStatus::Err, payload),
        IdcResponse::BudgetExceeded(payload) => (StInboundMsgResultStatus::BudgetExceeded, payload),
        IdcResponse::DeliveryError(error) => return Err(error.into()),
        IdcResponse::AlreadyAcked => {
            // We should have finished cleanup before acknowledging this message.
            // Check that here. This reply cannot replace a missing result or tell
            // us whether the reducer succeeded. Do not advance ack_prefix or call
            // on_result again.
            let tx = db.begin_tx(Workload::Internal);
            let retired = db
                .st_outbound_stream_by_id(&tx, msg.stream_id.0)?
                .is_some_and(|stream| msg.sequence <= stream.ack_prefix);
            anyhow::ensure!(
                retired,
                "receiver has retired the outcome for pending stream {}, sequence {}",
                msg.stream_id,
                msg.sequence,
            );
            return Ok(None);
        }
    };
    let result = MessageResult { status, payload };
    record_delivery_result(db, msg, &result)?;
    Ok(Some(result))
}

/// Run the callback and cleanup on the module instance, using the same transaction
/// checks as scheduled work. If the host has stopped, keep the saved result for restart.
async fn finalize_message(module_host: &WeakModuleHost, msg: DeliveredMessage) -> anyhow::Result<()> {
    let host = module_host
        .upgrade()
        .context("IDC module host is no longer available")?;
    host.run_idc_callback_and_cleanup(msg).await
}

/// Runs on the module executor. Check the row and run the callback in the same
/// transaction so the callback cannot run for a deleted or replaced row.
pub(super) fn finish_message_with_instance(
    info: &ModuleInfo,
    msg: DeliveredMessage,
    common: &mut InstanceCommon,
    inst: &mut impl WasmInstance,
) -> (anyhow::Result<()>, bool) {
    let db = info.relational_db();
    let sender_identity = info.database_identity.to_hex();
    let stream_id = msg.message.stream_id;
    let sequence = msg.message.sequence;
    let mut trapped = false;
    let result = (|| {
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        if !pending_message_exists(db, &tx, &msg.message)? {
            return Ok(());
        }

        let params = if current_row_matches(db, &tx, &msg.message)? {
            msg.message
                .on_result_reducer
                .as_deref()
                .map(|name| callback_params(info, &msg, name))
                .transpose()
        } else {
            Ok(None)
        };
        let params = match params {
            Ok(params) => params,
            Err(error) => {
                log::error!(
                    "idc_actor: sender_identity={sender_identity} stream_id={stream_id} sequence={sequence}: cannot prepare on_result: {error:#}"
                );
                None
            }
        };
        let Some(params) = params else {
            // No callback to run. Clean up the message in the open transaction.
            let (result, did_trap) = cleanup_message_with_views(tx, info, &msg, common, inst);
            trapped = did_trap;
            return result;
        };

        let name = info.module_def.reducer_by_id(params.reducer_id).name.clone();
        tx.ctx = ExecutionContext::with_workload(
            tx.ctx.database_identity(),
            Workload::Reducer(ReducerContext {
                name: name.clone(),
                caller_identity: params.caller_identity,
                caller_connection_id: params.caller_connection_id,
                timestamp: params.timestamp,
                arg_bsatn: params.args.get_bsatn().clone(),
            }),
        );

        // Successful callbacks commit their changes and cleanup together.
        let callback_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            common.call_reducer_with_success_action(Some(tx), params, inst, |tx, _| cleanup_message(db, tx, &msg))
        }));
        match callback_result {
            Ok((result, did_trap)) => {
                trapped = did_trap;
                if matches!(result.result.outcome, ReducerOutcome::Committed) {
                    return Ok(());
                }
                log::error!(
                    "idc_actor: sender_identity={sender_identity} stream_id={stream_id} sequence={sequence}: on_result {name} failed: {:?}",
                    result.result.outcome,
                );
            }
            Err(_) => {
                trapped = true;
                log::error!(
                    "idc_actor: sender_identity={sender_identity} stream_id={stream_id} sequence={sequence}: on_result {name} panicked"
                );
            }
        }

        // The callback transaction rolled back. Start a new transaction for cleanup.
        // If cleanup also fails, the message remains pending and a later attempt
        // can run the failed callback again.
        let cleanup_tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        let (result, cleanup_trapped) = cleanup_message_with_views(cleanup_tx, info, &msg, common, inst);
        trapped |= cleanup_trapped;
        result
    })();
    (result, trapped)
}

/// Commit cleanup and view updates without running the callback.
fn cleanup_message_with_views(
    mut tx: MutTxId,
    info: &ModuleInfo,
    msg: &DeliveredMessage,
    common: &mut InstanceCommon,
    inst: &mut impl WasmInstance,
) -> (anyhow::Result<()>, bool) {
    if let Err(error) = cleanup_message(info.relational_db(), &mut tx, msg) {
        return (Err(error), false);
    }
    // User-row cleanup must refresh views and notify subscriptions, just like
    // scheduled-row cleanup. Do not bypass the module's commit path.
    common.refresh_views_then_commit_and_broadcast(tx, inst)
}

fn callback_params(info: &ModuleInfo, msg: &DeliveredMessage, name: &str) -> anyhow::Result<CallReducerParams> {
    let mut bytes = spacetimedb_sats::bsatn::to_vec(&msg.message.outbox_row)?;
    let result: Result<(), String> = match msg.result.status {
        StInboundMsgResultStatus::Ok => Ok(()),
        StInboundMsgResultStatus::Err | StInboundMsgResultStatus::BudgetExceeded => {
            Err(String::from_utf8_lossy(&msg.result.payload).into_owned())
        }
    };
    spacetimedb_sats::bsatn::to_writer(&mut bytes, &result)?;
    let (id, def, owning) = info
        .module_def
        .reducer_by_name_with_module(name)
        .ok_or_else(|| anyhow::anyhow!("on_result reducer {name} not found"))?;
    let args = FunctionArgs::Bsatn(bytes.into()).into_tuple_for_def(owning, def)?;
    Ok(CallReducerParams::from_system(
        Timestamp::now(),
        Identity::ZERO,
        id,
        args,
    ))
}

fn pending_message_exists(db: &RelationalDB, tx: &MutTxId, msg: &PendingMessage) -> anyhow::Result<bool> {
    Ok(db
        .outbound_msg_by_stream_seq(tx, msg.stream_id.0, msg.sequence)?
        .is_some())
}

fn current_row_matches(db: &RelationalDB, tx: &MutTxId, msg: &PendingMessage) -> anyhow::Result<bool> {
    Ok(db
        .outbox_row(tx, msg.outbox_table_id, msg.message_id)?
        .is_some_and(|row| row == msg.outbox_row))
}

/// Repeated cleanup is a no-op. Only delete the row used for this delivery.
/// Leave replacement rows alone.
fn cleanup_message(db: &RelationalDB, tx: &mut MutTxId, delivered: &DeliveredMessage) -> anyhow::Result<()> {
    let msg = &delivered.message;
    if !pending_message_exists(db, tx, msg)? {
        return Ok(());
    }
    if current_row_matches(db, tx, msg)? {
        db.delete_by_rel(tx, msg.outbox_table_id, [msg.outbox_row.clone()]);
    }
    tx.ack_outbound_idc_msg(msg.stream_id.0, msg.sequence)?;
    Ok(())
}

/// Restore pending streams at startup without loading message bodies.
fn restore_pending_streams(db: &RelationalDB, streams: &mut DeliveryStreams) -> anyhow::Result<()> {
    for stream_id in read_pending_streams(db)? {
        streams.push(stream_id);
    }
    Ok(())
}

fn read_pending_streams(db: &RelationalDB) -> anyhow::Result<Vec<StreamId>> {
    let tx = db.begin_tx(Workload::Internal);
    let mut pending = Vec::new();
    for stream in db.st_outbound_streams(&tx)? {
        let stream = stream?;
        // next_seq - 1 is the last assigned sequence, so anything beyond ack_prefix is still pending.
        if stream.ack_prefix < stream.next_seq - 1 {
            pending.push(StreamId(stream.stream_id));
        }
    }
    Ok(pending)
}

/// Read ack_prefix + 1 for both delivery and callback processing.
/// Even with a saved result, this message must finish cleanup before the next one starts.
fn read_next_ordered_message(db: &RelationalDB, stream_id: StreamId) -> anyhow::Result<Option<LoadedMessage>> {
    let tx = db.begin_tx(Workload::Internal);
    // An insert may have rolled back or cleanup may have finished before we read its notification.
    let Some(stream) = db.st_outbound_stream_by_id(&tx, stream_id.0)? else {
        return Ok(None);
    };
    let seq = stream
        .ack_prefix
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("IDC stream {stream_id} sequence overflow"))?;
    if seq == stream.next_seq {
        return Ok(None);
    }
    anyhow::ensure!(
        seq < stream.next_seq,
        "IDC stream {stream_id} acknowledgment exceeds its sequence"
    );
    // A missing pending row is not an acknowledgment. Never skip a gap in the stream.
    let row = db
        .outbound_msg_by_stream_seq(&tx, stream_id.0, seq)?
        .ok_or_else(|| anyhow::anyhow!("IDC stream {stream_id} is missing pending sequence {seq}"))?;
    let row = StOutboundMsgRow::try_from(row)?;
    load_pending_message(db, tx, &stream, row).map(Some)
}

fn load_pending_message(
    db: &RelationalDB,
    tx: Tx,
    stream: &StOutboundStreamRow,
    message_row: StOutboundMsgRow,
) -> anyhow::Result<LoadedMessage> {
    let schema = db.schema_for_table(&tx, stream.outbox_table_id)?;
    let outbox = schema
        .outbox
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("table is no longer an outbox"))?;
    let outbox_row = db
        .outbox_row(&tx, stream.outbox_table_id, message_row.msg_id)?
        .ok_or_else(|| anyhow::anyhow!("pending IDC message has no outbox row {}", message_row.msg_id))?;
    let reducer_args = encode_reducer_args(&outbox_row, &outbox.arg_columns).into();
    db.release_tx(tx);

    let message = PendingMessage {
        stream_id: StreamId(message_row.stream_id),
        message_id: message_row.msg_id,
        sequence: message_row.seq,
        outbox_table_id: stream.outbox_table_id,
        target_identity: stream.target_identity.0,
        ack_prefix: stream.ack_prefix,
        target_reducer: outbox.remote_reducer.clone(),
        signature_hash: Hash::from_hex(&outbox.signature_hash)?,
        reducer_args,
        outbox_row,
        on_result_reducer: outbox.on_result_reducer.clone(),
    };

    match (message_row.result_status, message_row.result_payload) {
        (None, None) => Ok(LoadedMessage::Pending(message)),
        (Some(status), Some(payload)) => Ok(LoadedMessage::Delivered(DeliveredMessage {
            message,
            result: MessageResult { status, payload },
        })),
        _ => anyhow::bail!("pending IDC message has an incomplete stored result"),
    }
}

/// Encode the mapped columns as a BSATN reducer argument product.
///
/// `arg_columns` lists columns in receiver parameter order. BSATN products store fields
/// back to back, without names or a field count. Write only the mapped columns.
fn encode_reducer_args(row: &ProductValue, arg_columns: &[ColId]) -> Vec<u8> {
    let mut out = Vec::new();
    for col in arg_columns {
        let elem = row
            .elements
            .get(col.idx())
            .expect("validated outbox arg column should exist in row");
        spacetimedb_sats::bsatn::to_writer(&mut out, elem).expect("writing outbox row args to BSATN should never fail");
    }
    out
}

/// Send a request through localhost and let the host's proxy route it to the receiver.
///
/// The request body is the BSATN argument product in [`PendingMessage::reducer_args`].
/// Response bodies use the format described by [`MessageResult::payload`].
///
/// Read the full body before interpreting the HTTP status with [`IdcResponse`]. Errors leave
/// the message pending, even when someone needs to fix the underlying problem.
async fn attempt_delivery(
    client: &reqwest::Client,
    database_identity: Identity,
    config: &IdcConfig,
    msg: &PendingMessage,
) -> anyhow::Result<IdcResponse> {
    let token = config
        .token_signer
        .sign(database_identity, msg.target_identity)
        .context("IDC signing failed")?;
    let target_db_hex = msg.target_identity.to_hex();
    let url = format!(
        "http://localhost:{}/v1/database/{target_db_hex}/call-from-database/{}?stream_id={}&seq={}&ack_prefix={}",
        config.http_port, msg.target_reducer, msg.stream_id, msg.sequence, msg.ack_prefix,
    );
    let resp = client
        .post(&url)
        .query(&[("signature_hash", msg.signature_hash.to_hex().as_str())])
        .bearer_auth(token)
        .header("Content-Type", "application/octet-stream")
        .body(msg.reducer_args.clone())
        .send()
        .await?;

    let status = resp.status();
    let payload = resp
        .bytes()
        .await
        .with_context(|| format!("HTTP {status}: failed to read response"))?;
    Ok(IdcResponse::from_http_parts(status, payload))
}

/// Save the receiver's result and clear the last transport error. Keep the message
/// until its callback and acknowledgment are handled.
fn record_delivery_result(db: &RelationalDB, msg: &PendingMessage, result: &MessageResult) -> anyhow::Result<()> {
    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    tx.record_outbound_idc_result(msg.stream_id.0, msg.sequence, result.status, result.payload.clone())?;
    db.commit_tx(tx)?;
    Ok(())
}

/// Save the latest transport error and increment the retry count. Keep the message
/// pending even if saving the error fails.
fn record_transport_error(db: &RelationalDB, msg: &PendingMessage, reason: String) {
    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    if let Err(e) = tx.record_outbound_idc_transport_error(msg.stream_id.0, msg.sequence, reason) {
        log::error!(
            "idc_actor: sender_identity={} stream_id={} sequence={}: failed to record transport error: {e:#}",
            db.database_identity().to_hex(),
            msg.stream_id,
            msg.sequence,
        );
        let _ = db.rollback_mut_tx(tx);
    } else if let Err(e) = db.commit_tx(tx) {
        log::error!(
            "idc_actor: sender_identity={} stream_id={} sequence={}: failed to commit transport error: {e:#}",
            db.database_identity().to_hex(),
            msg.stream_id,
            msg.sequence,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Path, Query};
    use axum::http::StatusCode;
    use axum::routing::post;
    use axum::Router;
    use spacetimedb_datastore::system_tables::{
        StOutboundMsgRow, StOutboundStreamRow, ST_OUTBOUND_MSG_ID, ST_OUTBOUND_STREAM_ID,
    };
    use spacetimedb_lib::db::auth::{StAccess, StTableType};
    use spacetimedb_primitives::ColId;
    use spacetimedb_sats::bsatn::to_vec;
    use spacetimedb_sats::{product, AlgebraicType};
    use spacetimedb_schema::def::BTreeAlgorithm;
    use spacetimedb_schema::reducer_name::ReducerName;
    use spacetimedb_schema::schema::{ColumnSchema, ConstraintSchema, IndexSchema, OutboxSchema, TableSchema};
    use spacetimedb_schema::table_name::TableName;
    use spacetimedb_table::page_pool::PagePool;
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;
    use tokio::net::TcpListener;

    fn open_test_db() -> Arc<RelationalDB> {
        let (db, _) = RelationalDB::open(
            Identity::ZERO,
            Identity::ZERO,
            spacetimedb_durability::EmptyHistory::new(),
            None,
            None,
            PagePool::new_for_test(),
        )
        .expect("failed to open test database");
        Arc::new(db)
    }

    fn outbox_schema() -> TableSchema {
        TableSchema::new(
            TableId::SENTINEL,
            TableName::for_test("outbox"),
            None,
            vec![
                ColumnSchema::for_test(0, "target", AlgebraicType::identity()),
                ColumnSchema::for_test(1, "msg_id", AlgebraicType::U64),
                ColumnSchema::for_test(2, "value", AlgebraicType::U32),
            ],
            vec![IndexSchema::for_test("outbox_msg_id", BTreeAlgorithm::from(1))],
            vec![ConstraintSchema::unique_for_test("outbox_msg_id_unique", 1)],
            vec![],
            StTableType::User,
            StAccess::Public,
            None,
            Some(ColId(1)),
            false,
            None,
            Some(OutboxSchema {
                remote_reducer: ReducerName::for_test("receive_value"),
                target_column: ColId(0),
                arg_columns: vec![ColId(2)],
                on_result_reducer: None,
                signature_hash: spacetimedb_schema::def::reducer_signature_hash(
                    &spacetimedb_sats::ProductType::from([("value", AlgebraicType::U32)]),
                    &AlgebraicType::unit(),
                    &AlgebraicType::String,
                    &spacetimedb_sats::Typespace::default(),
                )
                .to_hex()
                .to_string(),
            }),
        )
    }

    fn insert_outbox_message(db: &RelationalDB, target: Identity, value: u32) -> anyhow::Result<TableId> {
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        let table_id = db.create_table(&mut tx, outbox_schema())?;
        db.commit_tx(tx)?;
        insert_outbox_row(db, table_id, target, 7, value)?;
        Ok(table_id)
    }

    fn insert_outbox_row(
        db: &RelationalDB,
        table_id: TableId,
        target: Identity,
        msg_id: u64,
        value: u32,
    ) -> anyhow::Result<()> {
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        let row = to_vec(&product![target, msg_id, value])?;
        let (row_ptr, insert_flags) = {
            let (_, row_ref, insert_flags) = db.insert(&mut tx, table_id, &row)?;
            (row_ref.pointer(), insert_flags)
        };
        assert!(insert_flags.is_outbox_table);
        tx.record_outbox_insert(table_id, row_ptr)?;
        db.commit_tx(tx)?;
        Ok(())
    }

    fn next_message(db: &RelationalDB) -> anyhow::Result<PendingMessage> {
        let streams = read_pending_streams(db)?;
        assert_eq!(streams.len(), 1);
        let LoadedMessage::Pending(message) = read_next_ordered_message(db, streams[0])?.unwrap() else {
            anyhow::bail!("expected a message without a result");
        };
        Ok(message)
    }

    // These tests cover delivery and queue state without a module instance.
    // They do not exercise callbacks, view refresh, or subscription updates.
    fn cleanup_without_module(db: &RelationalDB, msg: DeliveredMessage) -> anyhow::Result<()> {
        anyhow::ensure!(
            msg.message.on_result_reducer.is_none(),
            "test cleanup cannot run callbacks"
        );
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        cleanup_message(db, &mut tx, &msg)?;
        db.commit_tx(tx)?;
        Ok(())
    }

    fn store_success(db: &RelationalDB, message: PendingMessage) -> anyhow::Result<DeliveredMessage> {
        let result = MessageResult {
            status: StInboundMsgResultStatus::Ok,
            payload: Bytes::new(),
        };
        record_delivery_result(db, &message, &result)?;
        Ok(DeliveredMessage { message, result })
    }

    #[test]
    fn idc_actor_only_delivers_committed_inserts() -> anyhow::Result<()> {
        let db = open_test_db();
        let table = insert_outbox_message(&db, Identity::ONE, 42)?;
        let first = store_success(&db, next_message(&db)?)?;
        let (mut env, _runtime) = super::super::instance_env::test::instance_env(db.clone())?;
        // Modules can only insert outbox rows. Delivery must wait until the insert commits.
        let tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        let index = db.schema_for_table_mut(&tx, table)?.indexes[0].index_id;
        let (tx, result) = env.tx.clone().set(tx, || -> anyhow::Result<()> {
            // Reject writes even before delivery starts, and even when no row matches.
            for id in [7u64, 999] {
                let error = env
                    .update(table, index, &mut to_vec(&product![Identity::ONE, id, 99u32])?)
                    .unwrap_err();
                assert!(error.to_string().contains("outbox tables are insert-only"));
                assert!(env
                    .datastore_delete_by_index_scan_point_bsatn(index, &to_vec(&id)?)
                    .is_err());
            }
            let rows = vec![product![Identity::ONE, 7u64, 42u32]];
            assert!(env.datastore_delete_all_by_eq_bsatn(table, &to_vec(&rows)?).is_err());
            let start = to_vec(&std::ops::Bound::Included(7u64))?;
            let end = to_vec(&std::ops::Bound::Included(9u64))?;
            assert!(env
                .datastore_delete_by_index_scan_range_bsatn(index, &[], 0.into(), &start, &end)
                .is_err());
            assert!(env.clear(table).is_err());
            Ok(())
        });
        result?;
        let _ = db.rollback_mut_tx(tx);

        let (mut starter, sender) = IdcActor::open();
        env.idc_sender = sender;

        let tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        let (tx, result) = env.tx.clone().set(tx, || {
            env.insert(table, &mut to_vec(&product![Identity::ONE, 8u64, 43u32]).unwrap())
        });
        result?;
        let _ = db.rollback_mut_tx(tx);
        let msg = starter.rx.try_recv()?;
        let stream_id = StreamId(msg.stream_id);
        assert_eq!(
            read_next_ordered_message(&db, stream_id)?.unwrap().message().sequence,
            1
        );

        // After cleaning up the first message, the rolled-back insert leaves no work.
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        cleanup_message(&db, &mut tx, &first)?;
        db.commit_tx(tx)?;
        assert!(read_next_ordered_message(&db, stream_id)?.is_none());

        let tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        let (tx, result) = env.tx.clone().set(tx, || {
            env.insert(table, &mut to_vec(&product![Identity::ONE, 8u64, 43u32]).unwrap())
        });
        result?;
        db.commit_tx(tx)?;
        let msg = starter.rx.try_recv()?;
        assert_eq!(StreamId(msg.stream_id), stream_id);
        let next = read_next_ordered_message(&db, stream_id)?.unwrap();
        assert_eq!(next.message().message_id, 8);
        assert_eq!(next.message().sequence, 2);
        let next = store_success(&db, next_message(&db)?)?;
        // Cleanup can finish before an old notification is processed.
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        cleanup_message(&db, &mut tx, &next)?;
        cleanup_message(&db, &mut tx, &next)?;
        db.commit_tx(tx)?;
        assert!(read_next_ordered_message(&db, stream_id)?.is_none());
        assert!(read_pending_streams(&db)?.is_empty());

        // Rolling back the first insert of another stream leaves no stream to wake.
        let tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        let (tx, result) = env.tx.clone().set(tx, || {
            env.insert(table, &mut to_vec(&product![Identity::ZERO, 9u64, 44u32]).unwrap())
        });
        result?;
        let _ = db.rollback_mut_tx(tx);
        let msg = starter.rx.try_recv()?;
        assert!(read_next_ordered_message(&db, StreamId(msg.stream_id))?.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn idc_actor_cleanup_preserves_replacements_and_rejects_gaps() -> anyhow::Result<()> {
        let db = open_test_db();
        let table = insert_outbox_message(&db, Identity::ONE, 42)?;
        let msg = store_success(&db, next_message(&db)?)?;
        let replacement = product![Identity::ONE, 7u64, 99u32];

        // Internal writes can replace a row. Cleanup of the old message must leave it alone.
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        db.delete_by_rel(&mut tx, table, [msg.message.outbox_row.clone()]);
        db.insert(&mut tx, table, &to_vec(&replacement)?)?;
        assert!(!current_row_matches(&db, &tx, &msg.message)?);
        cleanup_message(&db, &mut tx, &msg)?;
        cleanup_message(&db, &mut tx, &msg)?;
        assert!(!pending_message_exists(&db, &tx, &msg.message)?);
        assert_eq!(db.iter_mut(&tx, table)?.next().unwrap().to_product_value(), replacement);
        db.commit_tx(tx)?;

        // Neither a missing user row nor a missing pending row permits skipping a sequence.
        for delete_pending in [false, true] {
            let db = open_test_db();
            let table = insert_outbox_message(&db, Identity::ONE, 42)?;
            insert_outbox_row(&db, table, Identity::ONE, 8, 43)?;
            let first = next_message(&db)?;
            let stream_id = first.stream_id;
            let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
            if delete_pending {
                let row = db
                    .outbound_msg_by_stream_seq(&tx, first.stream_id.0, first.sequence)?
                    .unwrap();
                let ptr = row.pointer();
                db.delete(&mut tx, ST_OUTBOUND_MSG_ID, [ptr]);
            } else {
                db.delete_by_rel(&mut tx, table, [first.outbox_row]);
            }
            db.commit_tx(tx)?;
            let mut streams = DeliveryStreams::default();
            streams.push(stream_id);
            assert!(take_ready_message(&db, &mut streams).is_err());
            assert_eq!(streams.next_retry(), None);
            let tx = db.begin_tx(Workload::Internal);
            assert_eq!(db.st_outbound_stream_by_id(&tx, stream_id.0)?.unwrap().ack_prefix, 0);
            assert!(db.outbound_msg_by_stream_seq(&tx, stream_id.0, 2)?.is_some());
            db.release_tx(tx);
        }
        Ok(())
    }

    #[tokio::test]
    async fn idc_actor_stores_complete_results_and_retries_transport_failures() -> anyhow::Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let keys = crate::auth::JwtKeys::generate()?;
        let response_status = Arc::new(std::sync::atomic::AtomicU16::new(200));
        let app = Router::new().route(
            "/v1/database/:target_db/call-from-database/:reducer",
            post({
                let response_status = response_status.clone();
                let public_key = keys.public;
                move |Path((target, reducer)): Path<(String, String)>,
                      Query(query): Query<HashMap<String, String>>,
                      headers: axum::http::HeaderMap,
                      body: Bytes| {
                    let status = response_status.load(Ordering::Relaxed);
                    let public_key = public_key.clone();
                    async move {
                        assert_eq!(target, Identity::ONE.to_hex().to_string());
                        assert_eq!(reducer, "receive_value");
                        let token = headers[axum::http::header::AUTHORIZATION].to_str().unwrap();
                        assert_eq!(
                            crate::auth::idc::verify(
                                &public_key,
                                token.strip_prefix("Bearer ").unwrap(),
                                Identity::ONE
                            )
                            .unwrap(),
                            Identity::ZERO
                        );
                        assert_eq!(query["seq"], "1");
                        assert_eq!(query["ack_prefix"], "0");
                        assert_eq!(query["signature_hash"], outbox_schema().outbox.unwrap().signature_hash);
                        assert_eq!(body.as_ref(), to_vec(&42u32).unwrap());
                        (StatusCode::from_u16(status).unwrap(), "receiver detail")
                    }
                }
            }),
        );
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let http_port = listener.local_addr()?.port();
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let mut config = IdcConfig {
            http_port,
            token_signer: Arc::new(keys.private),
        };
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(2))
            .build()?;

        // Save and reload each result without changing its status or payload.
        // Budget errors are stored results too.
        for (http_status, stored_status) in [
            (200, StInboundMsgResultStatus::Ok),
            (422, StInboundMsgResultStatus::Err),
            (402, StInboundMsgResultStatus::BudgetExceeded),
        ] {
            let db = open_test_db();
            let table = insert_outbox_message(&db, Identity::ONE, 42)?;
            insert_outbox_row(&db, table, Identity::ONE, 8, 43)?;
            let msg = next_message(&db)?;
            let seq = msg.sequence;
            response_status.store(http_status, Ordering::Relaxed);
            let delivered = deliver_message_and_record_result(&db, &client, Identity::ZERO, &config, msg)
                .await?
                .expect("terminal response must produce a delivered message");
            assert_eq!(delivered.result.status, stored_status);
            assert_eq!(delivered.result.payload.as_ref(), b"receiver detail");

            // A stored result does not advance the sequence until cleanup commits.
            let stored = read_next_ordered_message(&db, delivered.message.stream_id)?.unwrap();
            let LoadedMessage::Delivered(stored) = stored else {
                panic!("a stored result must load as a delivered message");
            };
            assert_eq!(stored.message.sequence, seq);
            assert_eq!(stored.result, delivered.result);
            {
                let tx = db.begin_tx(Workload::Internal);
                assert_eq!(db.iter(&tx, table)?.count(), 2);
                assert_eq!(
                    db.st_outbound_stream_by_id(&tx, stored.message.stream_id.0)?
                        .unwrap()
                        .ack_prefix,
                    0
                );
            }
            // Cleanup commits the saved result without another delivery attempt.
            cleanup_without_module(&db, stored)?;
            assert_eq!(next_message(&db)?.sequence, 2);
        }

        // Keep these messages pending and save the error. Do not acknowledge them
        // or run on_result.
        for status in [208, 401, 403, 404, 425, 500] {
            let db = open_test_db();
            insert_outbox_message(&db, Identity::ONE, 42)?;
            let msg = next_message(&db)?;
            response_status.store(status, Ordering::Relaxed);
            let error = deliver_message_and_record_result(&db, &client, Identity::ZERO, &config, msg)
                .await
                .err()
                .expect("response must leave the message pending");
            let tx = db.begin_tx(Workload::Internal);
            let row = StOutboundMsgRow::try_from(db.iter(&tx, ST_OUTBOUND_MSG_ID)?.next().unwrap())?;
            assert!(row.result_status.is_none(), "HTTP {status}");
            assert_eq!(row.retry_count, 1);
            let diagnostic = row.last_transport_error.unwrap();
            assert_eq!(diagnostic, format!("{error:#}"));
            if status == 208 {
                assert!(diagnostic.contains("receiver has retired the outcome"), "{diagnostic}");
            } else {
                assert!(diagnostic.contains(&format!("HTTP {status}")), "{diagnostic}");
                assert!(diagnostic.contains("receiver detail"), "{diagnostic}");
            }
            let stream = StOutboundStreamRow::try_from(db.iter(&tx, ST_OUTBOUND_STREAM_ID)?.next().unwrap())?;
            assert_eq!(stream.ack_prefix, 0);
            let _ = db.release_tx(tx);
        }
        // A late 208 must not repeat cleanup or the callback, or touch the next message.
        let db = open_test_db();
        let table = insert_outbox_message(&db, Identity::ONE, 42)?;
        insert_outbox_row(&db, table, Identity::ONE, 8, 43)?;
        let stale = next_message(&db)?;
        cleanup_without_module(&db, store_success(&db, stale.clone())?)?;
        response_status.store(208, Ordering::Relaxed);
        assert!(
            deliver_message_and_record_result(&db, &client, Identity::ZERO, &config, stale)
                .await?
                .is_none()
        );
        assert_eq!(next_message(&db)?.sequence, 2);
        {
            let tx = db.begin_tx(Workload::Internal);
            assert_eq!(db.iter(&tx, table)?.count(), 1);
            let row = StOutboundMsgRow::try_from(db.iter(&tx, ST_OUTBOUND_MSG_ID)?.next().unwrap())?;
            assert_eq!(row.retry_count, 0);
            assert!(row.result_status.is_none());
        }
        server.abort();
        let _ = server.await;
        // A terminal status alone is not enough. We need the whole response body.
        for status in [200, 422, 402] {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let http_port = listener.local_addr()?.port();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await?;
                let mut request = [0; 4096];
                let received = socket.read(&mut request).await?;
                anyhow::ensure!(received != 0, "client closed before sending a request");
                // Send the status, then close the connection before the body is complete.
                socket
                    .write_all(
                        format!("HTTP/1.1 {status} Test\r\nContent-Length: 100\r\nConnection: close\r\n\r\npartial")
                            .as_bytes(),
                    )
                    .await?;
                socket.shutdown().await?;
                anyhow::Ok(())
            });
            let db = open_test_db();
            insert_outbox_message(&db, Identity::ONE, 42)?;
            let msg = next_message(&db)?;
            config.http_port = http_port;
            assert!(
                deliver_message_and_record_result(&db, &client, Identity::ZERO, &config, msg)
                    .await
                    .is_err()
            );
            let tx = db.begin_tx(Workload::Internal);
            let row = StOutboundMsgRow::try_from(db.iter(&tx, ST_OUTBOUND_MSG_ID)?.next().unwrap())?;
            assert!(row.result_status.is_none());
            assert!(row.last_transport_error.unwrap().contains("failed to read response"));
            let stream = StOutboundStreamRow::try_from(db.iter(&tx, ST_OUTBOUND_STREAM_ID)?.next().unwrap())?;
            assert_eq!(stream.ack_prefix, 0);
            let _ = db.release_tx(tx);
            server.await??;
        }

        // A stored result requires both fields. Never resend or complete a partial result.
        for (result_status, result_payload) in [(Some(StInboundMsgResultStatus::Ok), None), (None, Some(Bytes::new()))]
        {
            let db = open_test_db();
            insert_outbox_message(&db, Identity::ONE, 42)?;
            let message = next_message(&db)?;
            let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
            let row = db
                .outbound_msg_by_stream_seq(&tx, message.stream_id.0, message.sequence)?
                .unwrap();
            let mut row = StOutboundMsgRow::try_from(row)?;
            db.delete_by_rel(&mut tx, ST_OUTBOUND_MSG_ID, [row.clone().into()]);
            row.result_status = result_status;
            row.result_payload = result_payload;
            db.insert(&mut tx, ST_OUTBOUND_MSG_ID, &to_vec(&row)?)?;
            db.commit_tx(tx)?;
            let error = read_next_ordered_message(&db, message.stream_id)
                .err()
                .expect("partial result must fail");
            assert!(error.to_string().contains("incomplete stored result"));
        }
        Ok(())
    }
}
