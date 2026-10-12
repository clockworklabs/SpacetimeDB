//! Inter-Database Communication (IDC) actor.
//!
//! # Row lifecycle
//!
//! Module writes enqueue work transactionally in two system tables. A row in
//! `st_outbound_stream` identifies an `(outbox table, receiver identity)` pair and
//! tracks sequence allocation and the sender's acknowledgement prefix. Each
//! `st_outbound_msg` row references a user outbox row by primary key and stores its
//! sequence, retry diagnostics, and eventual remote result. The payload itself is
//! read from the user table and encoded using the outbox's argument-column mapping.
//!
//! Acknowledging a message deletes its `st_outbound_msg` row and advances the stream's
//! `ack_prefix` in one transaction. The stream row survives for subsequent messages.
//! The actor does not delete the user outbox row; module code, including `on_result`,
//! owns that cleanup. If the payload row is no longer found before delivery, the
//! actor acknowledges the pending entry without sending it or calling `on_result`.
//! Deletion cannot recall an HTTP request that is already in flight.
//!
//! # Ordering and recovery
//!
//! The in-memory queues are a cache of durable work, loaded at startup and refreshed
//! after completions, notifications, and periodic polls. Notifications carry no
//! payload. Each stream processes its oldest queued sequence through delivery and
//! local finalization before starting another. Different streams progress concurrently
//! through `FuturesUnordered`, including while awaiting HTTP or a local result reducer.
//!
//! A retry uses the same stream id and sequence so the receiver can recognize a
//! duplicate delivery and replay its stored result. Once a remote result is persisted
//! locally, recovery can resume finalization without another HTTP request. Later
//! requests carry `ack_prefix`, allowing the receiver to discard acknowledged results;
//! there is no separate acknowledgement request when a stream becomes idle.
//!
//! # Results and failures
//!
//! Transport failures retain the message and back off only its stream, from 100 ms
//! up to 30 seconds, with no attempt limit. Remote success and reducer errors are terminal delivery results:
//! the actor records the result and invokes `on_result`, if configured, before acking.
//! A successful callback commits its own changes and the acknowledgement atomically.
//! An unavailable, failed, or noncommitting callback is logged and acknowledged in a
//! separate transaction; callback failures do not enter the transport retry loop.
//!
//! The standalone persistence helpers currently log write/commit failures rather than
//! return them to the delivery loop. Result recording is therefore not an enforced
//! barrier before finalization, and completing an in-memory attempt does not prove
//! that its acknowledgement committed. Unremoved system rows can be loaded again.

use crate::db::relational_db::RelationalDB;
use crate::host::module_host::WeakModuleHost;
use crate::host::wasm_common::module_host_actor::ReducerSuccessAction;
use crate::host::{FunctionArgs, ReducerOutcome};
use bytes::Bytes;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::system_tables::{
    StInboundMsgResultStatus, StOutboundMsgRow, StOutboundStreamRow, ST_OUTBOUND_MSG_ID, ST_OUTBOUND_STREAM_ID,
};
use spacetimedb_datastore::traits::IsolationLevel;
use spacetimedb_lib::{AlgebraicValue, Identity, ProductValue};
use spacetimedb_primitives::{ColId, TableId};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const INITIAL_BACKOFF: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

pub type IdcActorSender = mpsc::UnboundedSender<()>;

pub struct IdcActorConfig {
    pub sender_identity: Identity,
    pub http_port: u16,
}

pub struct IdcActor {
    _abort: tokio::task::AbortHandle,
}

pub struct IdcActorStarter {
    rx: mpsc::UnboundedReceiver<()>,
}

impl IdcActorStarter {
    pub fn start(self, db: Arc<RelationalDB>, config: IdcActorConfig, module_host: WeakModuleHost) -> IdcActor {
        let abort = tokio::spawn(run_idc_loop(db, config, Some(module_host), self.rx)).abort_handle();
        IdcActor { _abort: abort }
    }
}

impl IdcActor {
    pub fn open() -> (IdcActorStarter, IdcActorSender) {
        let (tx, rx) = mpsc::unbounded_channel();
        (IdcActorStarter { rx }, tx)
    }
}

/// Owned snapshot for one attempt; no row reference or read transaction crosses an await.
/// `request_row` is the full local row for `on_result`, while `args_bsatn` contains only
/// the mapped remote arguments. Idle queued snapshots are refreshed from the database.
#[derive(Clone)]
struct PendingMessage {
    st_row: StOutboundMsgRow,
    outbox_table_id: TableId,
    target_identity: Identity,
    ack_prefix: u64,
    target_reducer: String,
    signature_hash: Option<String>,
    args_bsatn: Bytes,
    request_row: ProductValue,
    on_result_reducer: Option<String>,
}

impl PendingMessage {
    fn stream_id(&self) -> u64 {
        self.st_row.stream_id
    }

    fn outbox_table_id(&self) -> TableId {
        self.outbox_table_id
    }

    fn msg_id(&self) -> u64 {
        self.st_row.msg_id
    }

    fn seq(&self) -> u64 {
        self.st_row.seq
    }

    fn target_identity(&self) -> Identity {
        self.target_identity
    }

    fn has_result(&self) -> bool {
        self.st_row.result_status.is_some()
    }
}

/// Transient scheduling state for one durable stream id. Retry timing is not persisted;
/// restarting the actor resets backoff, but preserves the message's stored diagnostics.
struct StreamQueue {
    queue: VecDeque<PendingMessage>,
    in_flight: bool,
    blocked_until: Option<Instant>,
    backoff: Duration,
}

impl StreamQueue {
    fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            in_flight: false,
            blocked_until: None,
            backoff: INITIAL_BACKOFF,
        }
    }

    fn is_ready(&self) -> bool {
        !self.in_flight && self.blocked_until.is_none_or(|until| Instant::now() >= until)
    }

    fn record_transport_error(&mut self) {
        self.blocked_until = Some(Instant::now() + self.backoff);
        self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
    }

    fn record_success(&mut self) {
        self.blocked_until = None;
        self.backoff = INITIAL_BACKOFF;
    }
}

enum DeliveryOutcome {
    Success(Bytes),
    ReducerError(Bytes),
    TransportError(String),
}

/// Launch ready stream heads and poll their completions alongside enqueue notifications
/// and retry deadlines. No single stream's network or callback await holds up this loop.
async fn run_idc_loop(
    db: Arc<RelationalDB>,
    config: IdcActorConfig,
    module_host: Option<WeakModuleHost>,
    mut notify_rx: mpsc::UnboundedReceiver<()>,
) {
    let client = reqwest::Client::builder()
        .http2_prior_knowledge()
        .http2_keep_alive_interval(Some(Duration::from_millis(200)))
        .http2_keep_alive_timeout(Duration::from_secs(5))
        .http2_keep_alive_while_idle(true)
        .http2_initial_connection_window_size(Some(2 * 64 * 1024))
        .http2_initial_stream_window_size(Some(64 * 1024))
        .timeout(Duration::from_secs(5))
        .build()
        .expect("failed to build IDC reqwest client");

    let mut streams: HashMap<u64, StreamQueue> = HashMap::new();
    let mut in_flight = FuturesUnordered::new();
    load_pending_into_streams(&db, &mut streams);

    loop {
        for (&stream_id, queue) in &mut streams {
            if !queue.is_ready() {
                continue;
            }
            let Some(msg) = queue.queue.front().cloned() else {
                continue;
            };

            // Keep the head queued until its entire attempt completes, including on_result.
            // Aborting the actor drops these futures, rather than leaving detached delivery
            // tasks. This does not undo remote work or a reducer already submitted to its host.
            queue.in_flight = true;
            let (db, client, config, module_host) = (&db, &client, &config, module_host.as_ref());
            in_flight.push(async move {
                let completed = process_message(db, client, config, module_host, msg).await;
                (stream_id, completed)
            });
        }

        let next_unblock = streams
            .values()
            .filter(|queue| !queue.in_flight && !queue.queue.is_empty())
            .filter_map(|queue| queue.blocked_until)
            .min()
            .map(|instant| instant.saturating_duration_since(Instant::now()));
        let sleep_duration = next_unblock.unwrap_or(POLL_INTERVAL).min(POLL_INTERVAL);

        tokio::select! {
            Some((stream_id, completed)) = in_flight.next(), if !in_flight.is_empty() => {
                let queue = streams.get_mut(&stream_id).expect("in-flight stream has a queue");
                queue.in_flight = false;
                if completed {
                    queue.queue.pop_front();
                    queue.record_success();
                } else {
                    queue.record_transport_error();
                }
            },
            _ = notify_rx.recv(), if !notify_rx.is_closed() => while notify_rx.try_recv().is_ok() {},
            _ = tokio::time::sleep(sleep_duration) => {},
        }

        load_pending_into_streams(&db, &mut streams);
    }
}

/// Process one stream head, retaining its system row through remote delivery and callback.
///
/// Return false only for a transport failure: the caller keeps the head and applies
/// per-stream backoff. True means finalization was attempted (or the payload was absent),
/// not that all persistence operations succeeded; those helpers log their own failures.
async fn process_message(
    db: &RelationalDB,
    client: &reqwest::Client,
    config: &IdcActorConfig,
    module_host: Option<&WeakModuleHost>,
    msg: PendingMessage,
) -> bool {
    if !outbox_row_exists(db, &msg) {
        log::debug!(
            "idc_actor: outbox row deleted for outbox_table_id={:?}, msg_id={}; acking outbound message",
            msg.outbox_table_id(),
            msg.msg_id(),
        );
        ack_message(db, msg.st_row);
        return true;
    }

    let outcome = if msg.has_result() {
        stored_delivery_outcome(&msg)
    } else {
        attempt_delivery(client, config, &msg).await
    };
    match outcome {
        DeliveryOutcome::TransportError(reason) => {
            log::warn!(
                "idc_actor: transport error delivering outbox_table_id={:?}, seq={} to {}: {reason}",
                msg.outbox_table_id(),
                msg.seq(),
                msg.target_identity().to_hex(),
            );
            record_transport_error(db, msg.st_row, reason);
            false
        }
        outcome => {
            if !msg.has_result() {
                let (status, payload) = outcome_to_result(&outcome);
                record_delivery_result(db, msg.st_row.clone(), status, payload);
            }
            finalize_message(db, module_host, msg, outcome).await;
            true
        }
    }
}

fn stored_delivery_outcome(msg: &PendingMessage) -> DeliveryOutcome {
    match msg.st_row.result_status.expect("checked by caller") {
        StInboundMsgResultStatus::Ok => DeliveryOutcome::Success(msg.st_row.result_payload.clone().unwrap_or_default()),
        StInboundMsgResultStatus::Err => {
            DeliveryOutcome::ReducerError(msg.st_row.result_payload.clone().unwrap_or_default())
        }
    }
}

fn outcome_to_result(outcome: &DeliveryOutcome) -> (StInboundMsgResultStatus, Bytes) {
    match outcome {
        DeliveryOutcome::Success(payload) => (StInboundMsgResultStatus::Ok, payload.clone()),
        DeliveryOutcome::ReducerError(payload) => (StInboundMsgResultStatus::Err, payload.clone()),
        DeliveryOutcome::TransportError(_) => unreachable!("transport errors are not final results"),
    }
}

/// Finish a terminal remote result without sending it again.
///
/// With no callback, acknowledge directly. Otherwise call the local reducer with the
/// captured outbox row followed by `Result<(), String>`: success maps to `Ok(())`, and
/// a remote reducer error becomes a lossily UTF-8-decoded error string. The remote
/// success payload is not passed to this callback.
///
/// This is an internal call (`Identity::ZERO`, no connection), not a call made with
/// the remote database's credentials. If it commits, `on_success` acknowledges within
/// that same reducer transaction. If invocation/encoding fails, the host is unavailable,
/// or the reducer does not commit, log and acknowledge separately instead of retrying
/// the callback. The next message in this stream waits until this function returns.
async fn finalize_message(
    db: &RelationalDB,
    module_host: Option<&WeakModuleHost>,
    msg: PendingMessage,
    outcome: DeliveryOutcome,
) {
    let Some(on_result_reducer) = &msg.on_result_reducer else {
        ack_message(db, msg.st_row);
        return;
    };

    let Some(host) = module_host.and_then(WeakModuleHost::upgrade) else {
        log::warn!(
            "idc_actor: module host gone, cannot call on_result reducer '{}' for outbox_table_id={:?}, seq={}",
            on_result_reducer,
            msg.outbox_table_id(),
            msg.seq(),
        );
        ack_message(db, msg.st_row);
        return;
    };

    let mut args_bytes = Vec::new();
    if let Err(e) = spacetimedb_sats::bsatn::to_writer(&mut args_bytes, &msg.request_row) {
        log::error!("idc_actor: failed to encode on_result request row: {e}");
        ack_message(db, msg.st_row);
        return;
    }

    let result_arg: Result<(), String> = match outcome {
        DeliveryOutcome::Success(_) => Ok(()),
        DeliveryOutcome::ReducerError(payload) => Err(String::from_utf8_lossy(&payload).into_owned()),
        DeliveryOutcome::TransportError(_) => unreachable!("transport errors are not finalized"),
    };
    if let Err(e) = spacetimedb_sats::bsatn::to_writer(&mut args_bytes, &result_arg) {
        log::error!("idc_actor: failed to encode on_result result arg: {e}");
        ack_message(db, msg.st_row);
        return;
    }

    let st_row = msg.st_row.clone();
    // This runs before the reducer transaction commits, so successful callback effects
    // and removal of the pending message cannot commit independently.
    let on_success: ReducerSuccessAction = Box::new(move |tx, _| {
        tx.ack_outbound_idc_msg(st_row)?;
        Ok(())
    });

    let result = host
        .call_reducer_with_success_action(
            Identity::ZERO,
            None,
            None,
            None,
            None,
            on_result_reducer,
            FunctionArgs::Bsatn(args_bytes.into()),
            on_success,
        )
        .await;

    match result {
        Ok(result) if matches!(result.outcome, ReducerOutcome::Committed) => {
            log::debug!(
                "idc_actor: on_result reducer '{}' called for outbox_table_id={:?}, seq={}",
                on_result_reducer,
                msg.outbox_table_id(),
                msg.seq(),
            );
        }
        Ok(result) => {
            log::error!(
                "idc_actor: on_result reducer '{}' did not commit for outbox_table_id={:?}, seq={}: {:?}",
                on_result_reducer,
                msg.outbox_table_id(),
                msg.seq(),
                result.outcome,
            );
            ack_message(db, msg.st_row);
        }
        Err(e) => {
            log::error!(
                "idc_actor: on_result reducer '{}' failed for outbox_table_id={:?}, seq={}: {e:?}",
                on_result_reducer,
                msg.outbox_table_id(),
                msg.seq(),
            );
            ack_message(db, msg.st_row);
        }
    }
}

/// Join pending system messages to their stream metadata and current user outbox rows.
///
/// Sort by sequence before adding new entries; refresh existing idle entries without
/// duplicating them or resetting backoff. Active streams keep their attempt snapshot
/// until completion. Missing streams/payloads are treated as orphaned work and acked;
/// missing schemas or outbox metadata are logged and left pending for a later scan.
fn load_pending_into_streams(db: &RelationalDB, streams: &mut HashMap<u64, StreamQueue>) {
    let tx = db.begin_tx(Workload::Internal);
    let stream_rows = db
        .iter(&tx, ST_OUTBOUND_STREAM_ID)
        .map(|iter| {
            iter.filter_map(|row_ref| StOutboundStreamRow::try_from(row_ref).ok())
                .map(|row| (row.stream_id, row))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_else(|e| {
            log::error!("idc_actor: failed to read st_outbound_stream: {e}");
            HashMap::new()
        });

    let st_rows = db
        .iter(&tx, ST_OUTBOUND_MSG_ID)
        .map(|iter| {
            iter.filter_map(|row_ref| StOutboundMsgRow::try_from(row_ref).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|e| {
            log::error!("idc_actor: failed to read st_outbound_msg: {e}");
            Vec::new()
        });

    let mut pending = Vec::with_capacity(st_rows.len());
    let mut orphaned = Vec::new();
    for st_row in st_rows {
        // Do not reload or acknowledge rows while this stream is delivering/finalizing.
        // It will be refreshed after completion, using the latest durable ack prefix.
        if streams.get(&st_row.stream_id).is_some_and(|queue| queue.in_flight) {
            continue;
        }
        let Some(stream) = stream_rows.get(&st_row.stream_id) else {
            log::error!(
                "idc_actor: cannot find outbound stream for stream_id={}, msg_id={}",
                st_row.stream_id,
                st_row.msg_id,
            );
            orphaned.push(st_row);
            continue;
        };
        let schema = match db.schema_for_table(&tx, stream.outbox_table_id) {
            Ok(schema) => schema,
            Err(e) => {
                log::error!(
                    "idc_actor: cannot find schema for outbox table {:?}, msg_id={}: {e}",
                    stream.outbox_table_id,
                    st_row.msg_id,
                );
                continue;
            }
        };
        let Some(outbox) = schema.outbox.as_ref() else {
            log::error!("idc_actor: table {:?} is not an outbox table", schema.table_name);
            continue;
        };
        let Some(msg_col) = schema.primary_key else {
            log::error!("idc_actor: outbox table {:?} has no primary key", schema.table_name);
            continue;
        };

        let outbox_row = db
            .iter_by_col_eq(
                &tx,
                stream.outbox_table_id,
                msg_col,
                &AlgebraicValue::U64(st_row.msg_id),
            )
            .ok()
            .and_then(|mut iter| iter.next());
        let Some(outbox_row) = outbox_row else {
            log::warn!(
                "idc_actor: outbox row not found in table {:?} for msg_id={}; acking orphaned outbound message",
                stream.outbox_table_id,
                st_row.msg_id,
            );
            orphaned.push(st_row);
            continue;
        };

        let request_row = outbox_row.to_product_value();
        let args_bsatn = encode_reducer_args(&request_row, &outbox.arg_columns);

        pending.push(PendingMessage {
            st_row,
            outbox_table_id: stream.outbox_table_id,
            target_identity: stream.target_identity.0,
            ack_prefix: stream.ack_prefix,
            target_reducer: outbox.remote_reducer.to_string(),
            signature_hash: Some(outbox.signature_hash.clone()),
            args_bsatn: args_bsatn.into(),
            request_row,
            on_result_reducer: outbox.on_result_reducer.as_ref().map(ToString::to_string),
        });
    }
    drop(tx);
    // Acknowledgements take a write transaction, so release the read transaction first.
    for st_row in orphaned {
        ack_message(db, st_row);
    }

    pending.sort_by_key(|msg| (msg.stream_id(), msg.seq()));
    for msg in pending {
        let queue = streams.entry(msg.stream_id()).or_insert_with(StreamQueue::new);
        if let Some(queued) = queue.queue.iter_mut().find(|queued| queued.seq() == msg.seq()) {
            *queued = msg;
        } else {
            queue.queue.push_back(msg);
        }
    }
}

/// Select columns in receiver parameter order, omitting local bookkeeping columns
/// unless they are explicitly included in the mapping.
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

/// Send one attempt through localhost, allowing the host's proxy to route the receiver.
///
/// The shared HTTP client has a five-second timeout. Request errors and HTTP statuses
/// other than 2xx/422/402 are retryable transport failures. HTTP 422 and 402 are terminal
/// reducer errors, while 2xx is success. Currently, failures reading a terminal response
/// body become an empty payload rather than changing that status into a transport error.
async fn attempt_delivery(client: &reqwest::Client, config: &IdcActorConfig, msg: &PendingMessage) -> DeliveryOutcome {
    let target_db_hex = msg.target_identity().to_hex();
    let mut url = format!(
        "http://localhost:{}/v1/database/{target_db_hex}/call-from-database/{}?stream_id={}&seq={}&ack_prefix={}",
        config.http_port,
        msg.target_reducer,
        msg.stream_id(),
        msg.seq(),
        msg.ack_prefix,
    );
    if let Some(signature_hash) = &msg.signature_hash {
        url.push_str("&signature_hash=");
        url.push_str(signature_hash);
    }

    let result = client
        .post(&url)
        .header("Content-Type", "application/octet-stream")
        .body(msg.args_bsatn.clone())
        .send()
        .await;

    match result {
        Err(e) => DeliveryOutcome::TransportError(e.to_string()),
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                DeliveryOutcome::Success(resp.bytes().await.unwrap_or_default())
            } else if status.as_u16() == 422 || status.as_u16() == 402 {
                DeliveryOutcome::ReducerError(resp.bytes().await.unwrap_or_default())
            } else {
                DeliveryOutcome::TransportError(format!("HTTP {status}"))
            }
        }
    }
}

fn outbox_row_exists(db: &RelationalDB, msg: &PendingMessage) -> bool {
    let tx = db.begin_tx(Workload::Internal);
    let schema = match db.schema_for_table(&tx, msg.outbox_table_id()) {
        Ok(schema) => schema,
        Err(_) => return false,
    };
    let Some(msg_col) = schema.primary_key else {
        return false;
    };
    db.iter_by_col_eq(&tx, msg.outbox_table_id(), msg_col, &AlgebraicValue::U64(msg.msg_id()))
        .ok()
        .and_then(|mut iter| iter.next())
        .is_some()
}

/// Store the terminal status/payload and clear the last transport error, retaining
/// the pending message for finalization. Write failures are logged, not propagated.
fn record_delivery_result(
    db: &RelationalDB,
    st_row: StOutboundMsgRow,
    result_status: StInboundMsgResultStatus,
    result_payload: Bytes,
) {
    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    if let Err(e) = tx.record_outbound_idc_result(st_row, result_status, result_payload) {
        log::error!("idc_actor: failed to record outbound IDC result: {e}");
        let _ = db.rollback_mut_tx(tx);
    } else if let Err(e) = db.commit_tx(tx) {
        log::error!("idc_actor: failed to commit outbound IDC result: {e}");
    }
}

/// Increment the durable retry count and save the latest error without acknowledging
/// the message. The caller schedules backoff even if this diagnostic write fails.
fn record_transport_error(db: &RelationalDB, st_row: StOutboundMsgRow, reason: String) {
    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    if let Err(e) = tx.record_outbound_idc_transport_error(st_row, reason) {
        log::error!("idc_actor: failed to record outbound IDC transport error: {e}");
        let _ = db.rollback_mut_tx(tx);
    } else if let Err(e) = db.commit_tx(tx) {
        log::error!("idc_actor: failed to commit outbound IDC transport error: {e}");
    }
}

/// Delete the system message and advance the stream prefix together, leaving the user
/// outbox row untouched. Used when acknowledgement is not part of a callback's commit.
/// Failures are logged; any system row left behind remains discoverable by the loader.
fn ack_message(db: &RelationalDB, st_row: StOutboundMsgRow) {
    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    if let Err(e) = tx.ack_outbound_idc_msg(st_row) {
        log::error!("idc_actor: failed to ack outbound IDC message: {e}");
        let _ = db.rollback_mut_tx(tx);
    } else if let Err(e) = db.commit_tx(tx) {
        log::error!("idc_actor: failed to commit outbound IDC ack: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Path, Query, State};
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::Router;
    use spacetimedb_datastore::system_tables::{
        StOutboundMsgRow, StOutboundStreamRow, ST_OUTBOUND_MSG_ID, ST_OUTBOUND_STREAM_ID,
    };
    use spacetimedb_lib::db::auth::{StAccess, StTableType};
    use spacetimedb_primitives::ColId;
    use spacetimedb_sats::bsatn::to_vec;
    use spacetimedb_sats::{product, AlgebraicType};
    use spacetimedb_schema::reducer_name::ReducerName;
    use spacetimedb_schema::schema::{ColumnSchema, OutboxSchema, TableSchema};
    use spacetimedb_schema::table_name::TableName;
    use spacetimedb_table::page_pool::PagePool;
    use std::collections::HashMap;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    #[derive(Debug)]
    struct CapturedRequest {
        target_db: String,
        reducer: String,
        query: HashMap<String, String>,
        body: Bytes,
    }

    async fn capture_idc_call(
        State(tx): State<mpsc::UnboundedSender<CapturedRequest>>,
        Path((target_db, reducer)): Path<(String, String)>,
        Query(query): Query<HashMap<String, String>>,
        body: Bytes,
    ) -> impl IntoResponse {
        tx.send(CapturedRequest {
            target_db,
            reducer,
            query,
            body,
        })
        .expect("test receiver should still be listening");
        (StatusCode::OK, Bytes::from_static(b"remote-ok"))
    }

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
            vec![],
            vec![],
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
                signature_hash: "receiver-signature".into(),
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

    #[test]
    fn idc_actor_encodes_outbox_args_in_declared_order() {
        let row = ProductValue::from(vec![
            AlgebraicValue::U64(10),
            AlgebraicValue::U32(20),
            AlgebraicValue::String("ignored".into()),
            AlgebraicValue::U32(30),
        ]);
        let bytes = encode_reducer_args(&row, &[ColId(3), ColId(1)]);
        let mut expected = Vec::new();
        spacetimedb_sats::bsatn::to_writer(&mut expected, &AlgebraicValue::U32(30)).unwrap();
        spacetimedb_sats::bsatn::to_writer(&mut expected, &AlgebraicValue::U32(20)).unwrap();

        assert_eq!(bytes, expected);
    }

    #[tokio::test]
    async fn idc_actor_delivers_outbox_message_and_acks_stream() -> anyhow::Result<()> {
        let (captured_tx, mut captured_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let app = Router::new()
            .route(
                "/v1/database/:target_db/call-from-database/:reducer",
                post(capture_idc_call),
            )
            .with_state(captured_tx);
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let http_port = listener.local_addr()?.port();
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await
        });

        let db = open_test_db();
        let target = Identity::ONE;
        insert_outbox_message(&db, target, 42)?;
        let tx = db.begin_tx(Workload::Internal);
        let stream_id = StOutboundStreamRow::try_from(db.iter(&tx, ST_OUTBOUND_STREAM_ID)?.next().unwrap())?.stream_id;
        let _ = db.release_tx(tx);
        let (_notify_tx, notify_rx) = mpsc::unbounded_channel();
        let actor = tokio::spawn(run_idc_loop(
            db.clone(),
            IdcActorConfig {
                sender_identity: Identity::ZERO,
                http_port,
            },
            None,
            notify_rx,
        ));

        let captured = tokio::time::timeout(Duration::from_secs(5), captured_rx.recv())
            .await?
            .expect("IDC actor should call the receiver");
        assert_eq!(captured.target_db, target.to_hex().to_string());
        assert_eq!(captured.reducer, "receive_value");
        assert_eq!(captured.query.get("stream_id"), Some(&stream_id.to_string()));
        assert_eq!(captured.query.get("seq").map(String::as_str), Some("1"));
        assert_eq!(captured.query.get("ack_prefix").map(String::as_str), Some("0"));
        assert_eq!(
            captured.query.get("signature_hash").map(String::as_str),
            Some("receiver-signature")
        );
        let request_row = product![target, 7u64, 42u32];
        assert_eq!(
            captured.body,
            Bytes::from(encode_reducer_args(&request_row, &[ColId(2)]))
        );

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let tx = db.begin_tx(Workload::Internal);
                let outbound_msgs = db
                    .iter(&tx, ST_OUTBOUND_MSG_ID)
                    .expect("can read st_outbound_msg")
                    .map(StOutboundMsgRow::try_from)
                    .collect::<Result<Vec<_>, _>>()
                    .expect("st_outbound_msg rows decode");
                let streams = db
                    .iter(&tx, ST_OUTBOUND_STREAM_ID)
                    .expect("can read st_outbound_stream")
                    .map(StOutboundStreamRow::try_from)
                    .collect::<Result<Vec<_>, _>>()
                    .expect("st_outbound_stream rows decode");
                let _ = db.release_tx(tx);

                if outbound_msgs.is_empty() && streams.len() == 1 && streams[0].ack_prefix == 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await?;

        actor.abort();
        let _ = shutdown_tx.send(());
        server.await??;
        Ok(())
    }

    async fn wait_for_ack(db: &RelationalDB, target: Identity, prefix: u64) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let tx = db.begin_tx(Workload::Internal);
                let acked = db.iter(&tx, ST_OUTBOUND_STREAM_ID).unwrap().any(|row| {
                    let stream = StOutboundStreamRow::try_from(row).unwrap();
                    stream.target_identity.0 == target && stream.ack_prefix == prefix
                });
                let _ = db.release_tx(tx);
                if acked {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        Ok(())
    }

    #[tokio::test]
    async fn idc_actor_streams_progress_independently_and_preserve_order() -> anyhow::Result<()> {
        let (captured_tx, mut captured_rx) = mpsc::unbounded_channel();
        let release_slow = Arc::new(tokio::sync::Semaphore::new(0));
        let fail_first = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let app = Router::new().route(
            "/v1/database/:target_db/call-from-database/:reducer",
            post({
                let release_slow = release_slow.clone();
                move |Path((target, _)): Path<(String, String)>, Query(query): Query<HashMap<String, String>>| {
                    let captured_tx = captured_tx.clone();
                    let release_slow = release_slow.clone();
                    let fail_first = fail_first.clone();
                    async move {
                        let seq = query["seq"].parse::<u64>().unwrap();
                        assert_eq!(query["ack_prefix"].parse::<u64>().unwrap(), seq - 1);
                        let slow = target == Identity::ONE.to_hex().to_string();
                        captured_tx.send((target, seq)).unwrap();
                        if slow && seq == 1 {
                            release_slow.acquire().await.unwrap().forget();
                            if fail_first.swap(false, std::sync::atomic::Ordering::Relaxed) {
                                return StatusCode::SERVICE_UNAVAILABLE;
                            }
                        }
                        StatusCode::OK
                    }
                }
            }),
        );
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let http_port = listener.local_addr()?.port();
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let server_abort = scopeguard::guard(server.abort_handle(), |handle| handle.abort());
        let db = open_test_db();
        let table_id = insert_outbox_message(&db, Identity::ONE, 42)?;
        insert_outbox_row(&db, table_id, Identity::ONE, 8, 43)?;
        let (notify_tx, notify_rx) = mpsc::unbounded_channel();
        let actor = tokio::spawn(run_idc_loop(
            db.clone(),
            IdcActorConfig {
                sender_identity: Identity::ZERO,
                http_port,
            },
            None,
            notify_rx,
        ));
        let actor_abort = scopeguard::guard(actor.abort_handle(), |handle| handle.abort());
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), captured_rx.recv()).await?,
            Some((Identity::ONE.to_hex().to_string(), 1)),
        );

        // Enqueue after the slow request starts: notifications and new streams must
        // still be serviced, but the second message of the slow stream must wait.
        insert_outbox_row(&db, table_id, Identity::ZERO, 9, 44)?;
        notify_tx.send(())?;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), captured_rx.recv()).await?,
            Some((Identity::ZERO.to_hex().to_string(), 1)),
        );
        wait_for_ack(&db, Identity::ZERO, 1).await?;

        insert_outbox_row(&db, table_id, Identity::ZERO, 10, 45)?;
        notify_tx.send(())?;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), captured_rx.recv()).await?,
            Some((Identity::ZERO.to_hex().to_string(), 2)),
        );
        wait_for_ack(&db, Identity::ZERO, 2).await?;
        wait_for_ack(&db, Identity::ONE, 0).await?;
        assert!(
            captured_rx.try_recv().is_err(),
            "slow stream must have only one request in flight"
        );

        release_slow.add_permits(1);
        // A transport error retries the same sequence, without blocking other streams.
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), captured_rx.recv()).await?,
            Some((Identity::ONE.to_hex().to_string(), 1)),
        );
        insert_outbox_row(&db, table_id, Identity::ZERO, 11, 46)?;
        notify_tx.send(())?;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), captured_rx.recv()).await?,
            Some((Identity::ZERO.to_hex().to_string(), 3)),
        );
        wait_for_ack(&db, Identity::ZERO, 3).await?;
        wait_for_ack(&db, Identity::ONE, 0).await?;

        release_slow.add_permits(1);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), captured_rx.recv()).await?,
            Some((Identity::ONE.to_hex().to_string(), 2)),
        );
        wait_for_ack(&db, Identity::ONE, 2).await?;
        drop(actor_abort);
        drop(server_abort);
        assert!(actor.await.unwrap_err().is_cancelled());
        assert!(server.await.unwrap_err().is_cancelled());
        Ok(())
    }
}
