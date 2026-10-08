/// Inter-Database Communication (IDC) Actor.
///
/// Background task that drains `st_outbound_msg`, delivers one in-flight message
/// per outbound stream, retries transport failures with per-stream backoff, and
/// runs the configured local result reducer before acknowledging the stream.
use crate::db::relational_db::RelationalDB;
use crate::host::module_host::WeakModuleHost;
use crate::host::wasm_common::module_host_actor::ReducerSuccessAction;
use crate::host::{FunctionArgs, ReducerOutcome};
use bytes::Bytes;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::system_tables::{
    StInboundMsgResultStatus, StOutboundMsgRow, StOutboundStreamRow, ST_OUTBOUND_MSG_ID, ST_OUTBOUND_STREAM_ID,
};
use spacetimedb_datastore::traits::IsolationLevel;
use spacetimedb_lib::{AlgebraicValue, Identity, ProductValue};
use spacetimedb_primitives::{ColId, TableId};
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
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

#[derive(Clone, Debug, Eq)]
struct DeliveryStreamKey {
    outbox_table_id: TableId,
    target_identity: Identity,
    target_reducer: String,
}

impl PartialEq for DeliveryStreamKey {
    fn eq(&self, other: &Self) -> bool {
        self.outbox_table_id == other.outbox_table_id
            && self.target_identity == other.target_identity
            && self.target_reducer == other.target_reducer
    }
}

impl Hash for DeliveryStreamKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.outbox_table_id.hash(state);
        self.target_identity.hash(state);
        self.target_reducer.hash(state);
    }
}

struct DatabaseQueue {
    queue: VecDeque<PendingMessage>,
    blocked_until: Option<Instant>,
    backoff: Duration,
}

impl DatabaseQueue {
    fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            blocked_until: None,
            backoff: INITIAL_BACKOFF,
        }
    }

    fn is_ready(&self) -> bool {
        self.blocked_until.is_none_or(|until| Instant::now() >= until)
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

    let mut db_queues: HashMap<DeliveryStreamKey, DatabaseQueue> = HashMap::new();
    load_pending_into_targets(&db, &mut db_queues);

    loop {
        let mut any_progress = false;
        for queue in db_queues.values_mut() {
            if !queue.is_ready() {
                continue;
            }
            let Some(msg) = queue.queue.front().cloned() else {
                continue;
            };

            if !outbox_row_exists(&db, &msg) {
                log::debug!(
                    "idc_actor: outbox row deleted for outbox_table_id={:?}, msg_id={}; acking outbound message",
                    msg.outbox_table_id(),
                    msg.msg_id(),
                );
                ack_message(&db, msg.st_row.clone());
                queue.queue.pop_front();
                queue.record_success();
                any_progress = true;
                continue;
            }

            let outcome = if msg.has_result() {
                stored_delivery_outcome(&msg)
            } else {
                attempt_delivery(&client, &config, &msg).await
            };

            match outcome {
                DeliveryOutcome::TransportError(reason) => {
                    log::warn!(
                        "idc_actor: transport error delivering outbox_table_id={:?}, seq={} to {}: {reason}",
                        msg.outbox_table_id(),
                        msg.seq(),
                        msg.target_identity().to_hex(),
                    );
                    record_transport_error(&db, msg.st_row.clone(), reason);
                    queue.record_transport_error();
                }
                outcome => {
                    if !msg.has_result() {
                        let (status, payload) = outcome_to_result(&outcome);
                        record_delivery_result(&db, msg.st_row.clone(), status, payload);
                    }
                    queue.queue.pop_front();
                    queue.record_success();
                    any_progress = true;
                    finalize_message(&db, module_host.as_ref(), msg, outcome).await;
                }
            }
        }

        if any_progress {
            load_pending_into_targets(&db, &mut db_queues);
            continue;
        }

        let next_unblock = db_queues
            .values()
            .filter_map(|queue| queue.blocked_until)
            .min()
            .map(|instant| instant.saturating_duration_since(Instant::now()));
        let sleep_duration = next_unblock.unwrap_or(POLL_INTERVAL).min(POLL_INTERVAL);

        tokio::select! {
            _ = notify_rx.recv() => while notify_rx.try_recv().is_ok() {},
            _ = tokio::time::sleep(sleep_duration) => {},
        }

        load_pending_into_targets(&db, &mut db_queues);
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

fn load_pending_into_targets(db: &RelationalDB, db_queues: &mut HashMap<DeliveryStreamKey, DatabaseQueue>) {
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
    for st_row in st_rows {
        let Some(stream) = stream_rows.get(&st_row.stream_id) else {
            log::error!(
                "idc_actor: cannot find outbound stream for stream_id={}, msg_id={}",
                st_row.stream_id,
                st_row.msg_id,
            );
            ack_message(db, st_row);
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
            ack_message(db, st_row);
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

    pending.sort_by_key(|msg| (msg.stream_id(), msg.seq()));
    for msg in pending {
        let stream_key = DeliveryStreamKey {
            outbox_table_id: msg.outbox_table_id(),
            target_identity: msg.target_identity(),
            target_reducer: msg.target_reducer.clone(),
        };
        let queue = db_queues.entry(stream_key).or_insert_with(DatabaseQueue::new);
        let already_queued = queue
            .queue
            .iter()
            .any(|queued| queued.stream_id() == msg.stream_id() && queued.seq() == msg.seq());
        if !already_queued {
            queue.queue.push_back(msg);
        }
    }
}

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

fn record_transport_error(db: &RelationalDB, st_row: StOutboundMsgRow, reason: String) {
    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    if let Err(e) = tx.record_outbound_idc_transport_error(st_row, reason) {
        log::error!("idc_actor: failed to record outbound IDC transport error: {e}");
        let _ = db.rollback_mut_tx(tx);
    } else if let Err(e) = db.commit_tx(tx) {
        log::error!("idc_actor: failed to commit outbound IDC transport error: {e}");
    }
}

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
        IdentityViaU256, StOutboundMsgRow, StOutboundStreamRow, ST_OUTBOUND_MSG_ID, ST_OUTBOUND_STREAM_ID,
    };
    use spacetimedb_lib::db::auth::{StAccess, StTableType};
    use spacetimedb_primitives::ColId;
    use spacetimedb_sats::bsatn::to_vec;
    use spacetimedb_sats::{product, AlgebraicType};
    use spacetimedb_schema::identifier::Identifier;
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
                ColumnSchema::for_test(0, "target", AlgebraicType::U256),
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
                remote_reducer: Identifier::new_unsafe_assume_valid("receive_value".into()),
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
        let row = to_vec(&product![IdentityViaU256(target), 7u64, value])?;
        let (row_ptr, insert_flags) = {
            let (_, row_ref, insert_flags) = db.insert(&mut tx, table_id, &row)?;
            (row_ref.pointer(), insert_flags)
        };
        assert!(insert_flags.is_outbox_table);
        tx.record_outbox_insert(table_id, row_ptr)?;
        db.commit_tx(tx)?;
        Ok(table_id)
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
        assert_eq!(captured.query.get("stream_id").map(String::as_str), Some("1"));
        assert_eq!(captured.query.get("seq").map(String::as_str), Some("1"));
        assert_eq!(captured.query.get("ack_prefix").map(String::as_str), Some("0"));
        assert_eq!(
            captured.query.get("signature_hash").map(String::as_str),
            Some("receiver-signature")
        );
        let request_row = product![IdentityViaU256(target), 7u64, 42u32];
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
}
