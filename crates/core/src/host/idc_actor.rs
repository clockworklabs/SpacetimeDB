/// Inter-Database Communication (IDC) Actor
///
/// Background tokio task that:
/// 1. Loads undelivered entries from `st_outbound_msg` on startup, resolving delivery data from outbox tables.
/// 2. Accepts immediate notifications via an mpsc channel when new outbox rows are inserted.
/// 3. Delivers each message in msg_id order via HTTP POST to
///    `http://localhost:{http_port}/v1/database/{target_db}/call-from-database/{reducer}?sender_identity=<hex>&msg_id=<n>`
/// 4. On transport errors (network, 5xx, 4xx except 422/402): retries infinitely with exponential
///    backoff, blocking only the affected target database (other targets continue unaffected).
/// 5. On reducer errors (HTTP 422) or budget exceeded (HTTP 402): calls the configured
///    `on_result_reducer` (read from the outbox table's schema) and deletes the st_outbound_msg row.
/// 6. Enforces sequential delivery per target database: msg N+1 is only delivered after N is done.
use crate::db::relational_db::RelationalDB;
use crate::host::module_host::WeakModuleHost;
use crate::host::FunctionArgs;
use bytes::Bytes;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::system_tables::{st_inbound_msg_result_status, StOutboundMsgRow, ST_OUTBOUND_MSG_ID};
use spacetimedb_datastore::traits::IsolationLevel;
use spacetimedb_lib::{AlgebraicValue, Identity, ProductValue};
use spacetimedb_primitives::{ColId, TableId};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const INITIAL_BACKOFF: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// How long to wait before polling again when there is no work.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// A sender that notifies the IDC actor of a new outbox row.
///
/// Sending `()` wakes the actor to deliver pending messages immediately
/// rather than waiting for the next poll cycle.
pub type IdcActorSender = mpsc::UnboundedSender<()>;

/// The identity of this (sender) database, set when the IDC actor is started.
pub struct IdcActorConfig {
    pub sender_identity: Identity,
    pub http_port: u16,
}

/// A handle that, when dropped, stops the IDC actor background task.
pub struct IdcActor {
    _abort: tokio::task::AbortHandle,
}

/// Holds the receiver side of the notification channel until the actor is started.
///
/// Mirrors the `SchedulerStarter` pattern: create the channel before the module is
/// loaded (so the sender can be stored in `InstanceEnv`), then call [`IdcActorStarter::start`]
/// once the DB is ready.
pub struct IdcActorStarter {
    rx: mpsc::UnboundedReceiver<()>,
}

impl IdcActorStarter {
    /// Spawn the IDC actor background task.
    pub fn start(self, db: Arc<RelationalDB>, config: IdcActorConfig, module_host: WeakModuleHost) -> IdcActor {
        let abort = tokio::spawn(run_idc_loop(db, config, module_host, self.rx)).abort_handle();
        IdcActor { _abort: abort }
    }
}

impl IdcActor {
    /// Open the IDC channel, returning a starter and a sender.
    ///
    /// Store the sender in `ModuleCreationContext` so it reaches `InstanceEnv`.
    /// After the module is ready, call [`IdcActorStarter::start`] to spawn the loop.
    pub fn open() -> (IdcActorStarter, IdcActorSender) {
        let (tx, rx) = mpsc::unbounded_channel();
        (IdcActorStarter { rx }, tx)
    }
}

/// All data needed to deliver a single outbound message, resolved from the outbox table.
#[derive(Clone)]
struct PendingMessage {
    msg_id: u64,
    /// Stored for future use (e.g. deleting the outbox row after delivery).
    #[allow(dead_code)]
    outbox_table_id: TableId,
    /// Stored for future use (e.g. deleting the outbox row after delivery).
    #[allow(dead_code)]
    row_id: u64,
    target_db_identity: Identity,
    target_reducer: String,
    args_bsatn: Vec<u8>,
    request_row: ProductValue,
    /// From the outbox table's `TableSchema::on_result_reducer`.
    on_result_reducer: Option<String>,
}

/// Per-target-database delivery state.
struct TargetState {
    queue: VecDeque<PendingMessage>,
    /// When `Some`, this target is in backoff and should not be retried until this instant.
    blocked_until: Option<Instant>,
    /// Current backoff duration for this target (doubles on each transport error).
    backoff: Duration,
}

impl TargetState {
    fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            blocked_until: None,
            backoff: INITIAL_BACKOFF,
        }
    }

    fn is_ready(&self) -> bool {
        match self.blocked_until {
            None => true,
            Some(until) => Instant::now() >= until,
        }
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

/// Outcome of a delivery attempt.
enum DeliveryOutcome {
    /// Reducer succeeded (HTTP 200).
    Success(Bytes),
    /// Reducer ran but returned Err (HTTP 422).
    ReducerError(String),
    /// Budget exceeded (HTTP 402).
    BudgetExceeded,
    /// Transport error: network failure, unexpected HTTP status, etc. Caller should retry.
    TransportError(String),
}

/// Main IDC actor loop: maintain per-target queues and deliver messages.
async fn run_idc_loop(
    db: Arc<RelationalDB>,
    config: IdcActorConfig,
    module_host: WeakModuleHost,
    mut notify_rx: mpsc::UnboundedReceiver<()>,
) {
    let client = reqwest::Client::new();

    // Per-target-database delivery state.
    let mut targets: HashMap<Identity, TargetState> = HashMap::new();

    // On startup, load any pending messages that survived a restart.
    load_pending_into_targets(&db, &mut targets);

    loop {
        // Deliver one message per ready target, then re-check.
        let mut any_delivered = true;
        while any_delivered {
            any_delivered = false;
            for state in targets.values_mut() {
                if !state.is_ready() {
                    continue;
                }
                let Some(msg) = state.queue.front().cloned() else {
                    continue;
                };
                let outcome = attempt_delivery(&client, &config, &msg).await;
                match outcome {
                    DeliveryOutcome::TransportError(reason) => {
                        log::warn!(
                            "idc_actor: transport error delivering msg_id={} to {}: {reason}",
                            msg.msg_id,
                            msg.target_db_identity.to_hex(),
                        );
                        state.record_transport_error();
                        // Do NOT pop the front — keep retrying this message for this target.
                    }
                    outcome => {
                        state.queue.pop_front();
                        state.record_success();
                        any_delivered = true;
                        let (result_status, result_payload) = outcome_to_result(&outcome);
                        finalize_message(&db, &module_host, &msg, result_status, result_payload).await;
                    }
                }
            }
        }

        // Compute how long to sleep: min over all blocked targets' unblock times.
        let next_unblock = targets
            .values()
            .filter_map(|s| s.blocked_until)
            .min()
            .map(|t| t.saturating_duration_since(Instant::now()));
        let sleep_duration = next_unblock.unwrap_or(POLL_INTERVAL).min(POLL_INTERVAL);

        // Wait for a notification or the next retry time.
        tokio::select! {
            _ = notify_rx.recv() => {
                // Drain all pending notifications (coalesce bursts).
                while notify_rx.try_recv().is_ok() {}
            }
            _ = tokio::time::sleep(sleep_duration) => {}
        }

        // Reload pending messages from DB (catches anything missed and handles restart recovery).
        load_pending_into_targets(&db, &mut targets);
    }
}

/// Decode the delivery outcome into `(result_status, result_payload)` for recording.
fn outcome_to_result(outcome: &DeliveryOutcome) -> (u8, Bytes) {
    match outcome {
        DeliveryOutcome::Success(payload) => (st_inbound_msg_result_status::SUCCESS, payload.clone()),
        DeliveryOutcome::ReducerError(msg) => (st_inbound_msg_result_status::REDUCER_ERROR, Bytes::from(msg.clone())),
        DeliveryOutcome::BudgetExceeded => (
            st_inbound_msg_result_status::REDUCER_ERROR,
            Bytes::from("budget exceeded".to_string()),
        ),
        DeliveryOutcome::TransportError(_) => unreachable!("transport errors never finalize"),
    }
}

/// Finalize a delivered message: call the on_result reducer (if any), then delete from ST_OUTBOUND_MSG.
///
/// On the happy path, `on_result_reducer` success and deletion of `st_outbound_msg`
/// are committed atomically in the same reducer transaction.
async fn finalize_message(
    db: &RelationalDB,
    module_host: &WeakModuleHost,
    msg: &PendingMessage,
    result_status: u8,
    result_payload: Bytes,
) {
    // Call the on_result reducer if configured.
    if let Some(on_result_reducer) = &msg.on_result_reducer {
        let Some(host) = module_host.upgrade() else {
            log::warn!(
                "idc_actor: module host gone, cannot call on_result reducer '{}' for msg_id={}",
                on_result_reducer,
                msg.msg_id,
            );
            delete_message(db, msg.msg_id);
            return;
        };

        let mut args_bytes = Vec::new();
        if let Err(e) = spacetimedb_sats::bsatn::to_writer(&mut args_bytes, &msg.request_row) {
            log::error!(
                "idc_actor: failed to encode on_result args for msg_id={}: {e}",
                msg.msg_id
            );
            delete_message(db, msg.msg_id);
            return;
        }
        match result_status {
            st_inbound_msg_result_status::SUCCESS => {
                args_bytes.push(0);
                args_bytes.extend_from_slice(&result_payload);
            }
            st_inbound_msg_result_status::REDUCER_ERROR => {
                let err = String::from_utf8_lossy(&result_payload).into_owned();
                if let Err(e) = spacetimedb_sats::bsatn::to_writer(&mut args_bytes, &Err::<(), String>(err)) {
                    log::error!(
                        "idc_actor: failed to encode on_result error args for msg_id={}: {e}",
                        msg.msg_id
                    );
                    delete_message(db, msg.msg_id);
                    return;
                }
            }
            status => {
                log::error!("idc_actor: unexpected result status {status} for msg_id={}", msg.msg_id);
                delete_message(db, msg.msg_id);
                return;
            }
        }

        let caller_identity = Identity::ZERO; // system call
        let result = host
            .call_reducer_delete_outbound_on_success(
                caller_identity,
                None, // no connection_id
                None, // no client sender
                None, // no request_id
                None, // no timer
                on_result_reducer,
                FunctionArgs::Bsatn(bytes::Bytes::from(args_bytes)),
                msg.msg_id,
            )
            .await;

        match result {
            Ok(_) => {
                log::debug!(
                    "idc_actor: on_result reducer '{}' called for msg_id={}",
                    on_result_reducer,
                    msg.msg_id,
                );
                return;
            }
            Err(e) => {
                log::error!(
                    "idc_actor: on_result reducer '{}' failed for msg_id={}: {e:?}",
                    on_result_reducer,
                    msg.msg_id,
                );
            }
        }
    }

    // Delete the row regardless of whether on_result succeeded or failed.
    delete_message(db, msg.msg_id);
}

/// Load all messages from ST_OUTBOUND_MSG into the per-target queues, resolving delivery data
/// from the corresponding outbox table rows.
///
/// A row's presence in ST_OUTBOUND_MSG means it has not yet been processed.
/// Messages already in a target's queue (by msg_id) are not re-added.
fn load_pending_into_targets(db: &RelationalDB, targets: &mut HashMap<Identity, TargetState>) {
    let tx = db.begin_tx(Workload::Internal);

    let st_outbound_msg_rows: Vec<StOutboundMsgRow> = db
        .iter(&tx, ST_OUTBOUND_MSG_ID)
        .map(|iter| {
            iter.filter_map(|row_ref| StOutboundMsgRow::try_from(row_ref).ok())
                .collect()
        })
        .unwrap_or_else(|e| {
            log::error!("idc_actor: failed to read st_outbound_msg: {e}");
            Vec::new()
        });

    let mut pending: Vec<PendingMessage> = Vec::with_capacity(st_outbound_msg_rows.len());

    for st_row in st_outbound_msg_rows {
        let outbox_table_id = TableId(st_row.outbox_table_id);

        // Read the outbox table schema for reducer name and on_result_reducer.
        let schema = match db.schema_for_table(&tx, outbox_table_id) {
            Ok(s) => s,
            Err(e) => {
                log::error!(
                    "idc_actor: cannot find schema for outbox table {:?} (msg_id={}): {e}",
                    outbox_table_id,
                    st_row.msg_id,
                );
                continue;
            }
        };

        let outbox_schema = match schema.outbox.as_ref() {
            Some(o) => o,
            None => {
                log::error!(
                    "idc_actor: table {:?} (msg_id={}) is not an outbox table",
                    schema.table_name,
                    st_row.msg_id,
                );
                continue;
            }
        };
        let target_reducer = outbox_schema.remote_reducer.to_string();
        let on_result_reducer = outbox_schema.on_result_reducer.as_ref().map(|id| id.to_string());

        // Look up the outbox row by its auto-inc PK (col 0) to get target identity and args.
        let outbox_row = db
            .iter_by_col_eq(&tx, outbox_table_id, ColId(0), &AlgebraicValue::U64(st_row.row_id))
            .ok()
            .and_then(|mut iter| iter.next());

        let Some(outbox_row_ref) = outbox_row else {
            log::error!(
                "idc_actor: outbox row not found in table {:?} for row_id={} (msg_id={})",
                outbox_table_id,
                st_row.row_id,
                st_row.msg_id,
            );
            continue;
        };

        let pv = outbox_row_ref.to_product_value();

        // Col 1: target_db_identity stored as SATS `Identity`,
        // i.e. the product wrapper `(__identity__: U256)`.
        let target_db_identity: Identity = match pv.elements.get(1) {
            Some(AlgebraicValue::Product(identity_pv)) if identity_pv.elements.len() == 1 => {
                match &identity_pv.elements[0] {
                    AlgebraicValue::U256(u) => Identity::from_u256(**u),
                    other => {
                        log::error!(
                            "idc_actor: outbox row col 1 expected Identity inner U256, got {other:?} (msg_id={})",
                            st_row.msg_id,
                        );
                        continue;
                    }
                }
            }
            other => {
                log::error!(
                    "idc_actor: outbox row col 1 expected Identity wrapper, got {other:?} (msg_id={})",
                    st_row.msg_id,
                );
                continue;
            }
        };

        // Cols 2+: args for the remote reducer.
        let args_bsatn = pv.elements[2..].iter().fold(Vec::new(), |mut acc, elem| {
            spacetimedb_sats::bsatn::to_writer(&mut acc, elem)
                .expect("writing outbox row args to BSATN should never fail");
            acc
        });

        pending.push(PendingMessage {
            msg_id: st_row.msg_id,
            outbox_table_id,
            row_id: st_row.row_id,
            target_db_identity,
            target_reducer,
            args_bsatn,
            request_row: pv,
            on_result_reducer,
        });
    }

    drop(tx);

    // Sort by msg_id ascending so delivery order is preserved.
    pending.sort_by_key(|m| m.msg_id);

    for msg in pending {
        let state = targets.entry(msg.target_db_identity).or_insert_with(TargetState::new);
        // Only add if not already in the queue (avoid duplicates after reload).
        let already_queued = state.queue.iter().any(|m| m.msg_id == msg.msg_id);
        if !already_queued {
            state.queue.push_back(msg);
        }
    }
}

/// Attempt a single HTTP delivery of a message.
async fn attempt_delivery(client: &reqwest::Client, config: &IdcActorConfig, msg: &PendingMessage) -> DeliveryOutcome {
    let target_db_hex = msg.target_db_identity.to_hex();
    let sender_hex = config.sender_identity.to_hex();

    let url = format!(
        "http://localhost:{}/v1/database/{target_db_hex}/call-from-database/{}?sender_identity={sender_hex}&msg_id={}",
        config.http_port, msg.target_reducer, msg.msg_id,
    );

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
                // HTTP 200: reducer committed successfully.
                DeliveryOutcome::Success(resp.bytes().await.unwrap_or_default())
            } else if status.as_u16() == 422 {
                // HTTP 422: reducer ran but returned Err(...).
                let body = resp.text().await.unwrap_or_default();
                DeliveryOutcome::ReducerError(body)
            } else if status.as_u16() == 402 {
                // HTTP 402: budget exceeded.
                DeliveryOutcome::BudgetExceeded
            } else {
                // Any other error (503, 404, etc.) is a transport error: retry.
                DeliveryOutcome::TransportError(format!("HTTP {status}"))
            }
        }
    }
}

/// Delete a message from ST_OUTBOUND_MSG within a new transaction.
fn delete_message(db: &RelationalDB, msg_id: u64) {
    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    if let Err(e) = tx.delete_outbound_msg(msg_id) {
        log::error!("idc_actor: failed to delete msg_id={msg_id}: {e}");
        let _ = db.rollback_mut_tx(tx);
        return;
    }
    if let Err(e) = db.commit_tx(tx) {
        log::error!("idc_actor: failed to commit delete for msg_id={msg_id}: {e}");
    }
}
