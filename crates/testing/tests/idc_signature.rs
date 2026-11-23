//! Signature failures are durable decisions, not transport failures. Retries must
//! replay those decisions even when the reducer schema has changed.
use spacetimedb::host::{FunctionArgs, IdcDeliveryFailure, IdcReducerCallOutcome};
use spacetimedb_lib::{hash_bytes, ConnectionId, Hash, Identity};
use spacetimedb_schema::def::reducer_signature_hash;
use spacetimedb_testing::modules::{CompilationMode, CompiledModule, DEFAULT_CONFIG, IN_MEMORY_CONFIG};

/// A duplicate must replay, not execute and fail while inserting its outcome.
#[test]
#[serial_test::serial]
fn concurrent_idc_deliveries_commit_once() {
    CompiledModule::compile("module-test", CompilationMode::Debug).with_module_async(
        IN_MEMORY_CONFIG,
        |handle| async move {
            let host = handle.client.module();
            let (_, reducer, _) = host.info.module_def.reducer_by_name_with_module("add").unwrap();
            let hash = host.info.reducer_signature_hash(&reducer.name);
            let args = spacetimedb_lib::bsatn::to_vec(&spacetimedb_lib::sats::product!["idc duplicate", 42u8]).unwrap();
            let calls = (0..8).map(|_| {
                host.call_idc_reducer(
                    Identity::ZERO,
                    ConnectionId::from_u128(1),
                    "add",
                    FunctionArgs::Bsatn(args.clone().into()),
                    1,
                    1,
                    0,
                    hash,
                )
            });
            let outcomes = tokio::time::timeout(std::time::Duration::from_secs(10), futures::future::join_all(calls))
                .await
                .unwrap();
            let mut applied = 0;
            for outcome in outcomes {
                match outcome.unwrap() {
                    IdcReducerCallOutcome::Recorded(result) => {
                        assert!(result.result.outcome.into_result().is_ok());
                        applied += 1;
                    }
                    IdcReducerCallOutcome::Replayed(result) => assert!(result.result_payload.is_empty()),
                    IdcReducerCallOutcome::AlreadyAcked => panic!("no request acknowledged this message"),
                }
            }
            assert_eq!(applied, 1);

            // add allocates a new row ID each time, so duplicate committed effects
            // would leave multiple rows even though the arguments are identical.
            let rows = spacetimedb::sql::execute::run(
                host.relational_db().clone(),
                "SELECT * FROM person WHERE name = 'idc duplicate'".into(),
                spacetimedb_lib::identity::AuthCtx::for_current(host.info.owner_identity),
                Some(host.info.subscriptions.clone()),
                Some(host.clone()),
                &mut vec![],
            )
            .await
            .unwrap();
            assert_eq!(rows.rows.len(), 1);
        },
    );
}

/// Pipelined deliveries can arrive out of order. An early one waits for its
/// predecessor instead of failing and sending the sender into backoff.
#[test]
#[serial_test::serial]
fn early_idc_delivery_waits_for_earlier_sequence() {
    CompiledModule::compile("module-test", CompilationMode::Debug).with_module_async(
        IN_MEMORY_CONFIG,
        |handle| async move {
            let host = handle.client.module();
            let (_, reducer, _) = host.info.module_def.reducer_by_name_with_module("say_hello").unwrap();
            let hash = host.info.reducer_signature_hash(&reducer.name);
            let call = |seq| {
                let host = host.clone();
                async move {
                    host.call_idc_reducer(
                        Identity::ZERO,
                        ConnectionId::from_u128(1),
                        "say_hello",
                        FunctionArgs::Nullary,
                        7,
                        seq,
                        0,
                        hash,
                    )
                    .await
                }
            };

            let second = tokio::spawn(call(2));
            // Give the second delivery time to find its predecessor missing.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!second.is_finished(), "an early delivery must wait");
            let IdcReducerCallOutcome::Recorded(first) = call(1).await.unwrap() else {
                panic!("the first delivery must run");
            };
            assert!(first.result.outcome.into_result().is_ok());
            let IdcReducerCallOutcome::Recorded(second) = second.await.unwrap().unwrap() else {
                panic!("the waiting delivery must run after its predecessor");
            };
            assert!(second.result.outcome.into_result().is_ok());

            // With no predecessor coming, the wait ends and the sender retries later.
            assert!(matches!(
                call(4).await,
                Err(IdcDeliveryFailure::OutOfOrder { expected: 3, actual: 4 })
            ));
        },
    );
}

/// A failed delivery records its error in a separate transaction. That must
/// also wake deliveries waiting behind it.
#[test]
#[serial_test::serial]
fn failed_idc_delivery_releases_waiting_successor() {
    CompiledModule::compile("module-test", CompilationMode::Debug).with_module_async(
        IN_MEMORY_CONFIG,
        |handle| async move {
            let host = handle.client.module();
            let call = |seq, reducer: &'static str, args: FunctionArgs| {
                let host = host.clone();
                async move {
                    let (_, def, _) = host.info.module_def.reducer_by_name_with_module(reducer).unwrap();
                    let hash = host.info.reducer_signature_hash(&def.name);
                    host.call_idc_reducer(
                        Identity::ZERO,
                        ConnectionId::from_u128(1),
                        reducer,
                        args,
                        9,
                        seq,
                        0,
                        hash,
                    )
                    .await
                }
            };

            let second = tokio::spawn(call(2, "say_hello", FunctionArgs::Nullary));
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!second.is_finished(), "an early delivery must wait");

            // There is no such row, so the reducer fails and its transaction rolls back.
            let args = spacetimedb_lib::bsatn::to_vec(&spacetimedb_lib::sats::product![u64::MAX]).unwrap();
            let IdcReducerCallOutcome::Recorded(first) = call(1, "delete_player", FunctionArgs::Bsatn(args.into()))
                .await
                .unwrap()
            else {
                panic!("the failure must be recorded");
            };
            assert!(first.result.outcome.into_result().is_err());

            let IdcReducerCallOutcome::Recorded(second) = second.await.unwrap().unwrap() else {
                panic!("the waiting delivery must run after its predecessor fails");
            };
            assert!(second.result.outcome.into_result().is_ok());
        },
    );
}

#[test]
#[serial_test::serial]
fn terminal_signature_errors_are_replayed_and_do_not_stall_the_stream() {
    CompiledModule::compile("module-test", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |handle| async move {
            let host = handle.client.module();
            let connection = ConnectionId::from_u128(1);
            let outdated = hash_bytes(b"outdated signature");
            let call = |name: &'static str, seq, hash: Hash| {
                let host = host.clone();
                async move {
                    host.call_idc_reducer(
                        Identity::ZERO,
                        connection,
                        name,
                        FunctionArgs::Bsatn(vec![255].into()),
                        42,
                        seq,
                        0,
                        hash,
                    )
                    .await
                }
            };

            // A later sequence cannot skip over the first undecided message,
            // even when that later message would fail signature validation.
            assert!(matches!(
                call("add", 2, outdated).await,
                Err(IdcDeliveryFailure::OutOfOrder { expected: 1, actual: 2 })
            ));

            // The first message has the wrong signature, so the receiver records a
            // terminal outcome for seq 1 instead of retrying it forever.
            let IdcReducerCallOutcome::Recorded(failed) = call("add", 1, outdated).await.unwrap() else {
                panic!("new message must record a terminal decision");
            };
            let error = failed.result.outcome.into_result().unwrap_err().to_string();
            assert!(error.contains("signature mismatch"), "{error}");
            let offset = failed.tx_offset.await.unwrap();

            // Retrying seq 1 replays the stored signature error despite a failed reducer lookup.
            // A changed route/schema and invalid arguments cannot replace the answer.
            let IdcReducerCallOutcome::Replayed(replayed) =
                call("removed_reducer", 1, hash_bytes(b"different signature"))
                    .await
                    .unwrap()
            else {
                panic!("decided message must replay despite a failed reducer lookup");
            };
            assert_eq!(&replayed.result_payload[..], error.as_bytes());
            assert!(
                replayed.tx_offset >= offset,
                "replay durability must cover the original decision"
            );

            // The next sequence can now advance and records its own terminal error.
            let IdcReducerCallOutcome::Recorded(missing) = call("removed_reducer", 2, outdated).await.unwrap() else {
                panic!("missing reducer must produce a terminal decision");
            };
            assert!(missing
                .result
                .outcome
                .into_result()
                .unwrap_err()
                .to_string()
                .contains("IDC reducer does not exist: removed_reducer"));

            let (_, reducer, owning) = host.info.module_def.reducer_by_name_with_module("add").unwrap();
            let hash = reducer_signature_hash(
                &reducer.params(),
                &reducer.ok_return_type,
                &reducer.err_return_type,
                owning.typespace(),
            );
            let IdcReducerCallOutcome::Recorded(invalid_args) = call("add", 3, hash).await.unwrap() else {
                panic!("invalid arguments must produce a terminal decision");
            };
            let error = invalid_args.result.outcome.into_result().unwrap_err().to_string();
            assert!(error.contains("invalid arguments for reducer add:"), "{error}");
            assert!(!error.contains("signature mismatch"), "{error}");

            // Matching hashes do not make malformed arguments valid. Keep the
            // decoding error so retries report the same failure.
            let IdcReducerCallOutcome::Replayed(replayed) = call("add", 3, hash).await.unwrap() else {
                panic!("invalid arguments must replay their recorded error");
            };
            assert_eq!(&replayed.result_payload[..], error.as_bytes());

            // Terminal errors for earlier messages must not stall a later valid delivery.
            let (_, reducer, owning) = host.info.module_def.reducer_by_name_with_module("say_hello").unwrap();
            let hash = reducer_signature_hash(
                &reducer.params(),
                &reducer.ok_return_type,
                &reducer.err_return_type,
                owning.typespace(),
            );
            let IdcReducerCallOutcome::Recorded(valid) = host
                .call_idc_reducer(
                    Identity::ZERO,
                    connection,
                    "say_hello",
                    FunctionArgs::Nullary,
                    42,
                    4,
                    3,
                    hash,
                )
                .await
                .unwrap()
            else {
                panic!("valid message following terminal errors must run");
            };
            assert!(valid.result.outcome.into_result().is_ok());
            let offset = valid.tx_offset.await.unwrap();

            // Successful outcomes are replayed too, and acked outcomes are not resurrected.
            let IdcReducerCallOutcome::Replayed(replayed) = call("removed_reducer", 4, outdated).await.unwrap() else {
                panic!("successful decisions must also survive schema changes");
            };
            assert!(replayed.result_payload.is_empty());
            assert!(replayed.tx_offset >= offset);
            assert!(matches!(
                call("removed_reducer", 1, outdated).await.unwrap(),
                IdcReducerCallOutcome::AlreadyAcked
            ));

            // Cleanup commits separately even when the accompanying delivery is
            // out of order and its sequence-check transaction is rolled back.
            assert!(matches!(
                host.call_idc_reducer(
                    Identity::ZERO,
                    connection,
                    "say_hello",
                    FunctionArgs::Nullary,
                    42,
                    6,
                    4,
                    hash,
                )
                .await,
                Err(IdcDeliveryFailure::OutOfOrder { expected: 5, actual: 6 })
            ));
            assert!(matches!(
                call("removed_reducer", 4, outdated).await.unwrap(),
                IdcReducerCallOutcome::AlreadyAcked
            ));
        },
    );
}
