/// Inter-Database Communication (IDC) Runtime
///
/// Background tokio task that:
/// 1. Loads Pending entries from `st_msg_id` on startup.
/// 2. Accepts immediate notifications via an mpsc channel when new outbox rows are inserted.
/// 3. Delivers each message in msg_id order via HTTP POST to
///    `http://localhost:80/v1/database/{target_db}/call-from-database/{reducer}?sender_identity=<hex>&msg_id=<n>`
/// 4. On transport errors (network, 5xx, 4xx except 422/402): retries infinitely with exponential
///    backoff, blocking only the affected target database (other targets continue unaffected).
/// 5. On reducer errors (HTTP 422) or budget exceeded (HTTP 402): records the result, marks DONE,
///    and calls the configured `on_result_reducer` on the local database (if any).
/// 6. Enforces sequential delivery per target database: msg N+1 is only delivered after N is done.
use crate::db::relational_db::RelationalDB;
use crate::host::module_host::WeakModuleHost;
use crate::host::FunctionArgs;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::system_tables::{StMsgIdRow, ST_MSG_ID_ID};
use spacetimedb_datastore::traits::IsolationLevel;
use spacetimedb_lib::Identity;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const IDC_HTTP_PORT: u16 = 80;
const INITIAL_BACKOFF: Duration = Duration::from_millis(100);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// How long to wait before polling again when there is no work.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// A sender that notifies the IDC runtime of a new outbox row.
///
/// Sending `()` wakes the runtime to deliver pending messages immediately
/// rather than waiting for the next poll cycle.
pub type IdcSender = mpsc::UnboundedSender<()>;

/// The identity of this (sender) database, set when the IDC runtime is started.
pub struct IdcRuntimeConfig {
    pub sender_identity: Identity,
}

/// A handle that, when dropped, stops the IDC runtime background task.
pub struct IdcRuntime {
    _abort: tokio::task::AbortHandle,
}

/// Holds the receiver side of the notification channel until the runtime is started.
///
/// Mirrors the `SchedulerStarter` pattern: create the channel before the module is
/// loaded (so the sender can be stored in `InstanceEnv`), then call [`IdcRuntimeStarter::start`]
/// once the DB is ready.
pub struct IdcRuntimeStarter {
    rx: mpsc::UnboundedReceiver<()>,
}

impl IdcRuntimeStarter {
    /// Spawn the IDC runtime background task.
    pub fn start(
        self,
        db: Arc<RelationalDB>,
        config: IdcRuntimeConfig,
        module_host: WeakModuleHost,
    ) -> IdcRuntime {
        let abort = tokio::spawn(run_idc_loop(db, config, module_host, self.rx)).abort_handle();
        IdcRuntime { _abort: abort }
    }
}

impl IdcRuntime {
    /// Open the IDC channel, returning a starter and a sender.
    ///
    /// Store the sender in `ModuleCreationContext` so it reaches `InstanceEnv`.
    /// After the module is ready, call [`IdcRuntimeStarter::start`] to spawn the loop.
    pub fn open() -> (IdcRuntimeStarter, IdcSender) {
        let (tx, rx) = mpsc::unbounded_channel();
        (IdcRuntimeStarter { rx }, tx)
    }
}

/// Per-target-database delivery state.
struct TargetState {
    queue: VecDeque<StMsgIdRow>,
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
    Success,
    /// Reducer ran but returned Err (HTTP 422).
    ReducerError(String),
    /// Budget exceeded (HTTP 402).
    BudgetExceeded,
    /// Transport error: network failure, unexpected HTTP status, etc. Caller should retry.
    TransportError(String),
}

/// Main IDC loop: maintain per-target queues and deliver messages.
async fn run_idc_loop(
    db: Arc<RelationalDB>,
    config: IdcRuntimeConfig,
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
                let Some(row) = state.queue.front().cloned() else {
                    continue;
                };
                let outcome = attempt_delivery(&client, &config, &row).await;
                match outcome {
                    DeliveryOutcome::TransportError(reason) => {
                        log::warn!(
                            "idc_runtime: transport error delivering msg_id={} to {}: {reason}",
                            row.msg_id,
                            hex::encode(Identity::from(row.target_db_identity).to_byte_array()),
                        );
                        state.record_transport_error();
                        // Do NOT pop the front — keep retrying this message for this target.
                    }
                    outcome => {
                        state.queue.pop_front();
                        state.record_success();
                        any_delivered = true;
                        let (result_status, result_payload) = outcome_to_result(&outcome);
                        finalize_message(&db, &module_host, &row, result_status, result_payload).await;
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
fn outcome_to_result(outcome: &DeliveryOutcome) -> (u8, String) {
    use spacetimedb_datastore::system_tables::st_inbound_msg_id_result_status;
    match outcome {
        DeliveryOutcome::Success => (st_inbound_msg_id_result_status::SUCCESS, String::new()),
        DeliveryOutcome::ReducerError(msg) => (st_inbound_msg_id_result_status::REDUCER_ERROR, msg.clone()),
        DeliveryOutcome::BudgetExceeded => (
            st_inbound_msg_id_result_status::REDUCER_ERROR,
            "budget exceeded".to_string(),
        ),
        DeliveryOutcome::TransportError(_) => unreachable!("transport errors never finalize"),
    }
}

/// Finalize a delivered message: call the on_result reducer (if any), then delete from ST_MSG_ID.
async fn finalize_message(
    db: &RelationalDB,
    module_host: &WeakModuleHost,
    row: &StMsgIdRow,
    _result_status: u8,
    result_payload: String,
) {
    // Call the on_result reducer if configured.
    if !row.on_result_reducer.is_empty() {
        let Some(host) = module_host.upgrade() else {
            log::warn!(
                "idc_runtime: module host gone, cannot call on_result reducer '{}' for msg_id={}",
                row.on_result_reducer,
                row.msg_id,
            );
            delete_message(db, row.msg_id);
            return;
        };

        // Encode (result_payload: String) as BSATN args.
        // The on_result reducer is expected to accept a single String argument.
        let args_bytes = match spacetimedb_sats::bsatn::to_vec(&result_payload) {
            Ok(b) => b,
            Err(e) => {
                log::error!(
                    "idc_runtime: failed to encode on_result args for msg_id={}: {e}",
                    row.msg_id
                );
                delete_message(db, row.msg_id);
                return;
            }
        };

        let caller_identity = Identity::ZERO; // system call
        let result = host
            .call_reducer(
                caller_identity,
                None, // no connection_id
                None, // no client sender
                None, // no request_id
                None, // no timer
                &row.on_result_reducer,
                FunctionArgs::Bsatn(bytes::Bytes::from(args_bytes)),
            )
            .await;

        match result {
            Ok(_) => {
                log::debug!(
                    "idc_runtime: on_result reducer '{}' called for msg_id={}",
                    row.on_result_reducer,
                    row.msg_id,
                );
            }
            Err(e) => {
                log::error!(
                    "idc_runtime: on_result reducer '{}' failed for msg_id={}: {e:?}",
                    row.on_result_reducer,
                    row.msg_id,
                );
            }
        }
    }

    // Delete the row regardless of whether on_result succeeded or failed.
    delete_message(db, row.msg_id);
}

/// Load all messages from ST_MSG_ID into the per-target queues.
///
/// A row's presence in the table means it has not yet been processed.
/// Messages that are already in a target's queue (by msg_id) are not re-added.
fn load_pending_into_targets(db: &RelationalDB, targets: &mut HashMap<Identity, TargetState>) {
    let tx = db.begin_tx(Workload::Internal);
    let rows: Vec<StMsgIdRow> = db
        .iter(&tx, ST_MSG_ID_ID)
        .map(|iter| iter.filter_map(|row_ref| StMsgIdRow::try_from(row_ref).ok()).collect())
        .unwrap_or_else(|e| {
            log::error!("idc_runtime: failed to read pending messages: {e}");
            Vec::new()
        });
    drop(tx);

    // Sort by msg_id ascending so delivery order is preserved.
    let mut sorted = rows;
    sorted.sort_by_key(|r| r.msg_id);

    for row in sorted {
        let target_id = Identity::from(row.target_db_identity);
        let state = targets.entry(target_id).or_insert_with(TargetState::new);
        // Only add if not already in the queue (avoid duplicates after reload).
        let already_queued = state.queue.iter().any(|r| r.msg_id == row.msg_id);
        if !already_queued {
            state.queue.push_back(row);
        }
    }
}

/// Attempt a single HTTP delivery of a message.
async fn attempt_delivery(
    client: &reqwest::Client,
    config: &IdcRuntimeConfig,
    row: &StMsgIdRow,
) -> DeliveryOutcome {
    let target_identity = Identity::from(row.target_db_identity);
    let target_db_hex = hex::encode(target_identity.to_byte_array());
    let sender_hex = hex::encode(config.sender_identity.to_byte_array());

    let url = format!(
        "http://localhost:{IDC_HTTP_PORT}/v1/database/{target_db_hex}/call-from-database/{}?sender_identity={sender_hex}&msg_id={}",
        row.target_reducer, row.msg_id,
    );

    let result = client
        .post(&url)
        .header("Content-Type", "application/octet-stream")
        .body(row.args_bsatn.clone())
        .send()
        .await;

    match result {
        Err(e) => DeliveryOutcome::TransportError(e.to_string()),
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() {
                // HTTP 200: reducer committed successfully.
                DeliveryOutcome::Success
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

/// Delete a message from ST_MSG_ID within a new transaction.
fn delete_message(db: &RelationalDB, msg_id: u64) {
    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    if let Err(e) = tx.delete_msg_id(msg_id) {
        log::error!("idc_runtime: failed to delete msg_id={msg_id}: {e}");
        let _ = db.rollback_mut_tx(tx);
        return;
    }
    if let Err(e) = db.commit_tx(tx) {
        log::error!("idc_runtime: failed to commit delete for msg_id={msg_id}: {e}");
    }
}
