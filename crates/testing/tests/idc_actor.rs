//! Exercise sender delivery and cleanup with a real module host.
//! The HTTP receiver is a stub that can delay requests or return errors.
//! This test does not run a receiver reducer or an `on_result` callback.
use axum::extract::{ConnectInfo, Path, Query};
use axum::http::{StatusCode, Version};
use axum::routing::post;
use axum::Router;
use serde::Deserialize;
use spacetimedb::db::relational_db::RelationalDB;
use spacetimedb::error::DatastoreError;
use spacetimedb::host::idc_actor::{self, IdcConfig};
use spacetimedb::host::ModuleHost;
use spacetimedb_datastore::execution_context::{Workload, WorkloadType};
use spacetimedb_datastore::system_tables::{StOutboundMsgRow, StOutboundStreamRow, ST_OUTBOUND_STREAM_ID};
use spacetimedb_datastore::traits::IsolationLevel;
use spacetimedb_lib::db::auth::{StAccess, StTableType};
use spacetimedb_lib::Identity;
use spacetimedb_primitives::{ColId, SequenceId, TableId};
use spacetimedb_sats::bsatn::to_vec;
use spacetimedb_sats::{product, AlgebraicType};
use spacetimedb_schema::def::BTreeAlgorithm;
use spacetimedb_schema::reducer_name::ReducerName;
use spacetimedb_schema::schema::{
    ColumnSchema, ConstraintSchema, IndexSchema, OutboxSchema, SequenceSchema, TableSchema,
};
use spacetimedb_schema::table_name::TableName;
use spacetimedb_testing::modules::{CompilationMode, CompiledModule, IN_MEMORY_CONFIG};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

// Exceed one notification batch to exercise draining while deliveries are active.
const NOTIFICATION_COUNT: usize = 64;
const TEST_TIMEOUT: Duration = Duration::from_secs(5);
const SLOW_TARGET: Identity = Identity::ONE;
const FAST_TARGET: Identity = Identity::ZERO;

#[derive(Deserialize)]
struct DeliveryQuery {
    seq: u64,
    ack_prefix: u64,
}

async fn next_delivery(rx: &mut mpsc::UnboundedReceiver<(String, u64)>) -> anyhow::Result<(String, u64)> {
    tokio::time::timeout(TEST_TIMEOUT, rx.recv())
        .await?
        .ok_or_else(|| anyhow::anyhow!("receiver stopped before the expected delivery"))
}

async fn expect_delivery(
    rx: &mut mpsc::UnboundedReceiver<(String, u64)>,
    target: Identity,
    message: &StOutboundMsgRow,
) -> anyhow::Result<()> {
    assert_eq!(next_delivery(rx).await?, (target.to_hex().to_string(), message.seq));
    Ok(())
}

/// Check startup recovery, per-stream ordering, retries, and insert notifications.
/// A blocked stream must not hold up other streams, and failed deliveries must
/// stay pending until the receiver accepts them, even after notifications close.
#[test]
#[serial_test::serial]
fn idc_actor_streams_progress_independently_and_preserve_order() {
    CompiledModule::compile("module-test", CompilationMode::Debug).with_module_async(
        IN_MEMORY_CONFIG,
        |module| async move {
            run_actor_test(module.client.module()).await.unwrap();
        },
    );
}

#[test]
#[serial_test::serial]
fn idc_actor_bounds_active_messages_and_reports_metrics() {
    CompiledModule::compile("module-test", CompilationMode::Debug).with_module_async(
        IN_MEMORY_CONFIG,
        |module| async move {
            run_bounded_actor_test(module.client.module()).await.unwrap();
        },
    );
}

async fn run_bounded_actor_test(host: ModuleHost) -> anyhow::Result<()> {
    use spacetimedb::worker_metrics::WORKER_METRICS;
    use spacetimedb_datastore::db_metrics::DB_METRICS;

    let identity = &host.info.database_identity;
    let transactions = DB_METRICS
        .rdb_num_txns
        .with_label_values(&WorkloadType::Idc, identity, "", &true);
    let initial_transactions = transactions.get();
    let db = host.relational_db().clone();
    let table = create_outbox(&db)?;
    let active = WORKER_METRICS.idc_in_flight_messages.with_label_values(identity);
    let bytes = WORKER_METRICS.idc_in_flight_bytes.with_label_values(identity);
    let queued = WORKER_METRICS.idc_notification_queue_depth.with_label_values(identity);
    let attempts = WORKER_METRICS
        .idc_delivery_attempts
        .with_label_values(identity, "outbox");
    let failures = WORKER_METRICS
        .idc_delivery_failures
        .with_label_values(identity, "outbox");
    let retries = WORKER_METRICS.idc_retries.with_label_values(identity, "outbox");
    let duration = WORKER_METRICS
        .idc_delivery_duration_seconds
        .with_label_values(identity, "outbox");
    let errors = WORKER_METRICS.idc_actor_errors.with_label_values(identity);
    let initial = (
        attempts.get(),
        failures.get(),
        retries.get(),
        duration.get_sample_count(),
        errors.get(),
    );

    let (requests_tx, mut requests_rx) = mpsc::unbounded_channel();
    let app = Router::new().route(
        "/v1/database/:target_db/call-from-database/:reducer",
        post(move || {
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel::<StatusCode>();
            requests_tx.send(reply_tx).unwrap();
            async move { reply_rx.await.unwrap() }
        }),
    );
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let config = IdcConfig {
        http_port: listener.local_addr()?.port(),
        token_signer: Arc::new(spacetimedb::auth::JwtKeys::generate()?.private),
    };
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let _server_abort = scopeguard::guard(server.abort_handle(), |handle| handle.abort());
    let (starter, notify_tx) = idc_actor::channel(identity);
    let count = idc_actor::MAX_IN_FLIGHT_MESSAGES + 2;
    for i in 0..count {
        let target = Identity::from_byte_array([i as u8; 32]);
        notify_tx.send(insert_outbox_row(&db, table, target)?)?;
    }
    assert_eq!(queued.get(), count as i64);
    let _actor = starter.start(
        db.clone(),
        *identity,
        idc_actor::http_client(),
        config.clone(),
        host.downgrade(),
    )?;

    let mut replies = Vec::new();
    for _ in 0..idc_actor::MAX_IN_FLIGHT_MESSAGES {
        replies.push(tokio::time::timeout(TEST_TIMEOUT, requests_rx.recv()).await?.unwrap());
    }
    assert!(tokio::time::timeout(Duration::from_millis(50), requests_rx.recv())
        .await
        .is_err());
    assert_eq!(active.get(), idc_actor::MAX_IN_FLIGHT_MESSAGES as i64);
    assert!(bytes.get() > 0);

    // A failed attempt frees a slot during backoff. Older queued streams get
    // their turn before the retry; completing another attempt frees the next slot.
    replies.pop().unwrap().send(StatusCode::SERVICE_UNAVAILABLE).unwrap();
    replies.push(tokio::time::timeout(TEST_TIMEOUT, requests_rx.recv()).await?.unwrap());
    assert_eq!(active.get(), idc_actor::MAX_IN_FLIGHT_MESSAGES as i64);
    replies.pop().unwrap().send(StatusCode::OK).unwrap();
    replies.push(tokio::time::timeout(TEST_TIMEOUT, requests_rx.recv()).await?.unwrap());
    assert_eq!(active.get(), idc_actor::MAX_IN_FLIGHT_MESSAGES as i64);

    for reply in replies {
        reply.send(StatusCode::OK).unwrap();
    }
    let retry = tokio::time::timeout(TEST_TIMEOUT, requests_rx.recv()).await?.unwrap();
    retry.send(StatusCode::OK).unwrap();
    for i in 0..count {
        wait_for_ack(&db, Identity::from_byte_array([i as u8; 32]), 1).await?;
    }
    notify_tx.close();
    tokio::time::timeout(TEST_TIMEOUT, notify_tx.closed()).await?;
    assert_eq!(active.get(), 0);
    assert_eq!(bytes.get(), 0);
    assert_eq!(queued.get(), 0);
    assert_eq!(attempts.get() - initial.0, count as u64 + 1);
    assert_eq!(failures.get() - initial.1, 1);
    assert_eq!(retries.get() - initial.2, 1);
    assert_eq!(duration.get_sample_count() - initial.3, count as u64 + 1);
    assert_eq!(errors.get(), initial.4);
    assert!(transactions.get() > initial_transactions);
    assert!(requests_rx.try_recv().is_err());

    // Aborting an actor also releases its gauge contributions, even while HTTP
    // is blocked. The message stays in the database for the next actor.
    let target = Identity::from_byte_array([174; 32]);
    let other_table = create_named_outbox(&db, "other_outbox")?;
    let other_attempts = WORKER_METRICS
        .idc_delivery_attempts
        .with_label_values(identity, "other_outbox");
    let initial_other_attempts = other_attempts.get();
    insert_outbox_row(&db, other_table, target)?;
    let (starter, notify_tx) = idc_actor::channel(identity);
    let actor = starter.start(
        db.clone(),
        *identity,
        idc_actor::http_client(),
        config,
        host.downgrade(),
    )?;
    let reply = tokio::time::timeout(TEST_TIMEOUT, requests_rx.recv()).await?.unwrap();
    assert_eq!(active.get(), 1);
    assert!(bytes.get() > 0);
    assert_eq!(other_attempts.get() - initial_other_attempts, 1);
    assert_eq!(attempts.get() - initial.0, count as u64 + 1);
    drop(actor);
    tokio::time::timeout(TEST_TIMEOUT, notify_tx.closed()).await?;
    assert_eq!(active.get(), 0);
    assert_eq!(bytes.get(), 0);
    assert_eq!(queued.get(), 0);
    wait_for_ack(&db, target, 0).await?;
    let _ = reply.send(StatusCode::OK);
    Ok(())
}

fn outbox_schema(name: &str) -> TableSchema {
    TableSchema::new(
        TableId::SENTINEL,
        TableName::for_test(name),
        None,
        vec![
            ColumnSchema::for_test(0, "target", AlgebraicType::identity()),
            ColumnSchema::for_test(1, "msg_id", AlgebraicType::U64),
            ColumnSchema::for_test(2, "value", AlgebraicType::U32),
        ],
        vec![IndexSchema::for_test(format!("{name}_msg_id"), BTreeAlgorithm::from(1))],
        vec![ConstraintSchema::unique_for_test(format!("{name}_msg_id_unique"), 1)],
        vec![SequenceSchema {
            sequence_id: SequenceId::SENTINEL,
            sequence_name: format!("{name}_msg_id_seq").into(),
            table_id: TableId::SENTINEL,
            col_pos: ColId(1),
            start: 1,
        }],
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
            ),
        }),
    )
}

fn create_outbox(db: &RelationalDB) -> anyhow::Result<TableId> {
    create_named_outbox(db, "outbox")
}

fn create_named_outbox(db: &RelationalDB, name: &str) -> anyhow::Result<TableId> {
    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    let table_id = db.create_table(&mut tx, outbox_schema(name))?;
    db.commit_tx(tx)?;
    Ok(table_id)
}

/// Commit a message without notifying the actor, so each scenario controls when it wakes.
fn insert_outbox_row(db: &RelationalDB, table_id: TableId, target: Identity) -> anyhow::Result<StOutboundMsgRow> {
    let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
    // Zero asks the sequence to allocate the message ID, just like a module insert.
    // The payload is identical so messages are distinguished by their allocated IDs.
    let row = to_vec(&product![target, 0u64, 42u32])?;
    let (row_ptr, insert_flags, msg_id) = {
        let (_, row_ref, insert_flags) = db.insert(&mut tx, table_id, &row)?;
        (row_ref.pointer(), insert_flags, row_ref.read_col::<u64>(ColId(1))?)
    };
    assert_ne!(msg_id, 0);
    assert!(insert_flags.is_outbox_table);
    let message = tx.record_outbox_insert(table_id, row_ptr)?;
    assert_eq!(message.msg_id, msg_id);
    db.commit_tx(tx)?;
    Ok(message)
}

/// Wait for sender cleanup to commit, not just for the HTTP stub to receive a request.
async fn wait_for_ack(db: &RelationalDB, target: Identity, prefix: u64) -> anyhow::Result<()> {
    tokio::time::timeout(TEST_TIMEOUT, async {
        loop {
            let tx = db.begin_tx(Workload::Internal);
            let acked = db.iter(&tx, ST_OUTBOUND_STREAM_ID).unwrap().any(|row| {
                let stream = StOutboundStreamRow::try_from(row.unwrap()).unwrap();
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

async fn run_actor_test(host: ModuleHost) -> anyhow::Result<()> {
    let keys = spacetimedb::auth::JwtKeys::generate()?;
    let (captured_tx, mut captured_rx) = mpsc::unbounded_channel();
    // The slow target blocks its first sequence and fails the first attempt.
    // The fast target accepts immediately.
    // A third target returns 403 until the test allows it to recover.
    let release_slow = Arc::new(tokio::sync::Semaphore::new(0));
    let fail_first = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let rejected_target = Identity::from_byte_array([2; 32]);
    let reject_auth = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let app = Router::new().route(
        "/v1/database/:target_db/call-from-database/:reducer",
        post({
            let release_slow = release_slow.clone();
            let reject_auth = reject_auth.clone();
            move |Path((target, _)): Path<(String, String)>, Query(query): Query<DeliveryQuery>| {
                let captured_tx = captured_tx.clone();
                let release_slow = release_slow.clone();
                let fail_first = fail_first.clone();
                let reject_auth = reject_auth.clone();
                async move {
                    let seq = query.seq;
                    // Every new sequence must carry acknowledgment of all earlier messages.
                    // Retrying the same sequence must not advance that acknowledgment.
                    assert_eq!(query.ack_prefix, seq - 1);
                    let slow = target == SLOW_TARGET.to_hex().to_string();
                    let rejected =
                        target == rejected_target.to_hex().to_string() && reject_auth.load(Ordering::Relaxed);
                    // Report arrival before blocking so the test can observe work still in flight.
                    captured_tx.send((target, seq)).unwrap();
                    if rejected {
                        return StatusCode::FORBIDDEN;
                    }
                    if slow && seq == 1 {
                        release_slow.acquire().await.unwrap().forget();
                        if fail_first.swap(false, Ordering::Relaxed) {
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
    let db = host.relational_db().clone();
    // Both messages predate actor startup and have no notifications. Startup must
    // discover the stream, but only send its first sequence.
    let table_id = create_outbox(&db)?;
    let slow_first = insert_outbox_row(&db, table_id, SLOW_TARGET)?;
    let slow_second = insert_outbox_row(&db, table_id, SLOW_TARGET)?;
    assert_eq!(slow_first.seq, 1);
    assert_eq!(slow_second.seq, slow_first.seq + 1);
    let (starter, notify_tx) = idc_actor::channel(&host.info.database_identity);
    let config = IdcConfig {
        http_port,
        token_signer: Arc::new(keys.private),
    };
    // Use the real host to commit cleanup through the view and subscription path.
    let actor = starter.start(
        db.clone(),
        host.info.database_identity,
        idc_actor::http_client(),
        config,
        host.downgrade(),
    )?;
    expect_delivery(&mut captured_rx, SLOW_TARGET, &slow_first).await?;

    // Insert more work while one request is waiting. Other streams should still
    // make progress, but the next message in the slow stream must wait.
    let slow_third = insert_outbox_row(&db, table_id, SLOW_TARGET)?;
    // Repeated notifications must not start a second attempt for the busy stream.
    for _ in 0..NOTIFICATION_COUNT {
        notify_tx.send(slow_third.clone())?;
    }
    let fast_first = insert_outbox_row(&db, table_id, FAST_TARGET)?;
    notify_tx.send(fast_first.clone())?;
    expect_delivery(&mut captured_rx, FAST_TARGET, &fast_first).await?;
    wait_for_ack(&db, FAST_TARGET, fast_first.seq).await?;

    // The fast stream is idle after cleanup. A fresh insert must wake it again.
    let fast_second = insert_outbox_row(&db, table_id, FAST_TARGET)?;
    notify_tx.send(fast_second.clone())?;
    expect_delivery(&mut captured_rx, FAST_TARGET, &fast_second).await?;
    wait_for_ack(&db, FAST_TARGET, fast_second.seq).await?;
    wait_for_ack(&db, SLOW_TARGET, 0).await?;
    assert!(
        captured_rx.try_recv().is_err(),
        "slow stream must have only one request in flight"
    );

    // Release the first attempt with 503. The actor must retry the same message,
    // while the fast stream keeps making progress.
    release_slow.add_permits(1);
    expect_delivery(&mut captured_rx, SLOW_TARGET, &slow_first).await?;
    let fast_third = insert_outbox_row(&db, table_id, FAST_TARGET)?;
    notify_tx.send(fast_third.clone())?;
    expect_delivery(&mut captured_rx, FAST_TARGET, &fast_third).await?;
    wait_for_ack(&db, FAST_TARGET, fast_third.seq).await?;
    wait_for_ack(&db, SLOW_TARGET, 0).await?;

    // Let the retry succeed. Cleanup must unlock the second message without a
    // notification for it, then the third message from the earlier insert.
    release_slow.add_permits(1);
    expect_delivery(&mut captured_rx, SLOW_TARGET, &slow_second).await?;
    expect_delivery(&mut captured_rx, SLOW_TARGET, &slow_third).await?;
    wait_for_ack(&db, SLOW_TARGET, slow_third.seq).await?;

    // An auth rejection leaves the message pending. Inserts and
    // duplicate notifications must not let later messages overtake it.
    let first_rejected = insert_outbox_row(&db, table_id, rejected_target)?;
    notify_tx.send(first_rejected.clone())?;
    expect_delivery(&mut captured_rx, rejected_target, &first_rejected).await?;
    let second_rejected = insert_outbox_row(&db, table_id, rejected_target)?;
    for _ in 0..NOTIFICATION_COUNT {
        notify_tx.send(first_rejected.clone())?;
        notify_tx.send(second_rejected.clone())?;
    }
    expect_delivery(&mut captured_rx, rejected_target, &first_rejected).await?;

    // Closing the channel must still drain every batch and finish its deliveries.
    let slow_last = insert_outbox_row(&db, table_id, SLOW_TARGET)?;
    for _ in 0..NOTIFICATION_COUNT {
        notify_tx.send(slow_last.clone())?;
    }
    let fast_last = insert_outbox_row(&db, table_id, FAST_TARGET)?;
    notify_tx.send(fast_last.clone())?;
    drop(notify_tx);
    // Independent streams may finish in either order. Reject duplicate deliveries,
    // but allow the rejected stream to keep retrying while the others finish.
    let mut expected = HashSet::from([
        (SLOW_TARGET.to_hex().to_string(), slow_last.seq),
        (FAST_TARGET.to_hex().to_string(), fast_last.seq),
    ]);
    tokio::time::timeout(TEST_TIMEOUT, async {
        while !expected.is_empty() {
            let delivered = next_delivery(&mut captured_rx).await?;
            if delivered.0 == rejected_target.to_hex().to_string() {
                assert_eq!(
                    delivered.1, first_rejected.seq,
                    "rejected stream must keep retrying its first sequence"
                );
            } else {
                assert!(expected.remove(&delivered), "unexpected delivery: {delivered:?}");
            }
        }
        anyhow::Ok(())
    })
    .await??;
    wait_for_ack(&db, SLOW_TARGET, slow_last.seq).await?;
    wait_for_ack(&db, FAST_TARGET, fast_last.seq).await?;
    // Only the rejected stream's rows should remain. Its 403 is a transport error,
    // not a stored reducer result, and must not advance ack_prefix.
    let tx = db.begin_tx(Workload::Internal);
    let remaining_ids = db
        .iter(&tx, table_id)?
        .map(|row| {
            let row = row?;
            Ok(row.read_col::<u64>(ColId(1))?)
        })
        .collect::<Result<HashSet<_>, DatastoreError>>()?;
    assert_eq!(
        remaining_ids,
        HashSet::from([first_rejected.msg_id, second_rejected.msg_id])
    );
    let stream = db
        .st_outbound_streams(&tx)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .find(|stream| stream.target_identity.0 == rejected_target)
        .unwrap();
    assert_eq!(stream.ack_prefix, 0);
    let row = db
        .outbound_msg_by_stream_seq(&tx, stream.stream_id, first_rejected.seq)?
        .unwrap();
    let row = StOutboundMsgRow::try_from(row)?;
    assert!(row.result_status.is_none());
    assert!(row.retry_count >= 1);
    assert!(row.last_transport_error.unwrap().contains("HTTP 403"));
    let _ = db.release_tx(tx);

    // Fixing authentication resumes delivery without restarting the actor or
    // sending another notification, even though the channel is now closed.
    reject_auth.store(false, Ordering::Relaxed);
    tokio::time::timeout(TEST_TIMEOUT, async {
        while let Some((target, seq)) = captured_rx.recv().await {
            assert_eq!(target, rejected_target.to_hex().to_string());
            assert!(seq == first_rejected.seq || seq == second_rejected.seq);
            if seq == second_rejected.seq {
                return;
            }
        }
        panic!("recovered stream must deliver its second message");
    })
    .await?;
    wait_for_ack(&db, rejected_target, second_rejected.seq).await?;
    // Both formerly rejected messages have now completed cleanup.
    let tx = db.begin_tx(Workload::Internal);
    assert_eq!(db.iter(&tx, table_id)?.count(), 0);
    let _ = db.release_tx(tx);
    drop(actor);
    drop(server_abort);
    assert!(server.await.unwrap_err().is_cancelled());
    Ok(())
}

#[test]
#[serial_test::serial]
fn idc_actor_shutdown_drains_attempts_without_dispatching_more() {
    CompiledModule::compile("module-test", CompilationMode::Debug).with_module_async(
        IN_MEMORY_CONFIG,
        |module| async move {
            run_shutdown_test(module.client.module()).await.unwrap();
        },
    );
}

async fn run_shutdown_test(host: ModuleHost) -> anyhow::Result<()> {
    let client = idc_actor::http_client();
    let mut connections = HashSet::new();
    let (requests_tx, mut requests_rx) = mpsc::unbounded_channel();
    let app = Router::new().route(
        "/v1/database/:target_db/call-from-database/:reducer",
        post(
            move |ConnectInfo(peer): ConnectInfo<SocketAddr>, version: Version, Query(query): Query<DeliveryQuery>| {
                let (reply_tx, reply_rx) = tokio::sync::oneshot::channel::<StatusCode>();
                assert_eq!(version, Version::HTTP_2);
                requests_tx.send((query.seq, reply_tx, peer)).unwrap();
                async move { reply_rx.await.unwrap() }
            },
        ),
    );
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let config = IdcConfig {
        http_port: listener.local_addr()?.port(),
        token_signer: Arc::new(spacetimedb::auth::JwtKeys::generate()?.private),
    };
    let app = app.into_make_service_with_connect_info::<SocketAddr>();
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let _server_abort = scopeguard::guard(server.abort_handle(), |handle| handle.abort());
    let db = host.relational_db().clone();
    let table_id = create_outbox(&db)?;
    let first = insert_outbox_row(&db, table_id, SLOW_TARGET)?;
    let second = insert_outbox_row(&db, table_id, SLOW_TARGET)?;

    // A shutdown already requested at startup must not dispatch recovered messages.
    {
        let (starter, notify_tx) = idc_actor::channel(&host.info.database_identity);
        notify_tx.close();
        let _actor = starter.start(
            db.clone(),
            host.info.database_identity,
            client.clone(),
            config.clone(),
            host.downgrade(),
        )?;
        tokio::time::timeout(TEST_TIMEOUT, notify_tx.closed()).await?;
        assert!(requests_rx.try_recv().is_err());
    }

    // A failed attempt remains pending. Restart retries it; a successful attempt
    // finishes local cleanup before shutdown returns, leaving the next row pending.
    for status in [StatusCode::SERVICE_UNAVAILABLE, StatusCode::OK] {
        let (starter, notify_tx) = idc_actor::channel(&host.info.database_identity);
        let _actor = starter.start(
            db.clone(),
            host.info.database_identity,
            client.clone(),
            config.clone(),
            host.downgrade(),
        )?;
        let (seq, reply, peer) = tokio::time::timeout(TEST_TIMEOUT, requests_rx.recv()).await?.unwrap();
        connections.insert(peer);
        assert_eq!(seq, first.seq);
        {
            // ModuleHost::exit signals closure through its sender and waits for
            // completion. Multiple waiters must all observe the completed drain.
            notify_tx.close();
            let mut shutdown = Box::pin(async {
                tokio::join!(notify_tx.closed(), notify_tx.closed());
            });
            assert!(
                tokio::time::timeout(Duration::from_millis(50), shutdown.as_mut())
                    .await
                    .is_err(),
                "shutdown returned before the active HTTP request completed"
            );
            reply.send(status).unwrap();
            tokio::time::timeout(TEST_TIMEOUT, shutdown).await?;
        }
        // Repeated shutdown is harmless.
        notify_tx.close();
        tokio::time::timeout(TEST_TIMEOUT, notify_tx.closed()).await?;
        assert!(
            requests_rx.try_recv().is_err(),
            "shutdown must not dispatch another attempt"
        );
        let tx = db.begin_tx(Workload::Internal);
        let stream = db.st_outbound_stream_by_id(&tx, first.stream_id)?.unwrap();
        if status == StatusCode::OK {
            assert_eq!(stream.ack_prefix, first.seq);
            assert!(db
                .outbound_msg_by_stream_seq(&tx, first.stream_id, first.seq)?
                .is_none());
        } else {
            assert_eq!(stream.ack_prefix, 0);
            let row = db.outbound_msg_by_stream_seq(&tx, first.stream_id, first.seq)?.unwrap();
            let row = StOutboundMsgRow::try_from(row)?;
            assert_eq!(row.retry_count, 1);
            assert!(row.result_status.is_none());
        }
        assert!(db
            .outbound_msg_by_stream_seq(&tx, second.stream_id, second.seq)?
            .is_some());
        let _ = db.release_tx(tx);
    }

    // Startup discovers the untouched second message without a notification.
    let (starter, notify_tx) = idc_actor::channel(&host.info.database_identity);
    let _actor = starter.start(
        db.clone(),
        host.info.database_identity,
        client,
        config,
        host.downgrade(),
    )?;
    let (seq, reply, peer) = tokio::time::timeout(TEST_TIMEOUT, requests_rx.recv()).await?.unwrap();
    connections.insert(peer);
    assert_eq!(
        connections.len(),
        1,
        "replacement actors must reuse the HTTP/2 connection"
    );
    assert_eq!(seq, second.seq);
    reply.send(StatusCode::OK).unwrap();
    wait_for_ack(&db, SLOW_TARGET, second.seq).await?;
    notify_tx.close();
    tokio::time::timeout(TEST_TIMEOUT, notify_tx.closed()).await?;
    tokio::time::timeout(TEST_TIMEOUT, host.exit()).await?;
    tokio::time::timeout(TEST_TIMEOUT, host.exited()).await?;
    Ok(())
}
