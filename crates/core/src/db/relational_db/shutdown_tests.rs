use super::*;
use crate::db::environment;
use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent, ModuleFunctionCall};
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use tests_utils::TestDB;

#[test]
fn operation_drain_shutdown_serializes_transactions_and_survives_cancelled_waiter() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let fixture = TestDB::durable().unwrap();
    let db = fixture.db.clone();
    runtime.block_on(async {
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Sql);
        environment::set(&db, &mut tx, "BEFORE", "committed").unwrap();
        let closing = tokio::spawn({
            let db = db.clone();
            async move { db.shutdown().await }
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while db.shutdown.get().is_none() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        closing.abort();
        assert!(closing.await.unwrap_err().is_cancelled());
        assert!(!db.commits_closed.load(Ordering::Relaxed));
        // This SQL/view transaction acquired the exclusive lock before shutdown.
        let (_, _, read) = db.commit_tx_downgrade(tx, Workload::Sql).unwrap();
        let _ = db.release_tx(read);
        assert!(db.shutdown().await.is_some());
        assert!(db.commits_closed.load(Ordering::Relaxed));
        for downgrade in [false, true] {
            let mut late = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Subscribe);
            environment::set(&db, &mut late, "LATE", "must-roll-back").unwrap();
            let error = if downgrade {
                db.commit_tx_downgrade(late, Workload::Subscribe).err().unwrap()
            } else {
                db.commit_tx(late).err().unwrap()
            };
            assert!(matches!(error, DBError::DatabaseClosed));
        }
        let read = db.begin_tx(Workload::Internal);
        assert_eq!(environment::get(&read, "BEFORE").unwrap().as_deref(), Some("committed"));
        assert_eq!(environment::get(&read, "LATE").unwrap(), None);
        let _ = db.release_tx(read);
    });
}

#[test]
fn operation_drain_retained_sql_and_subscription_handles_reject_after_writer_close() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let fixture = TestDB::durable().unwrap();
    let db = fixture.db.clone();
    runtime.block_on(async {
        let subscriptions = ModuleSubscriptions::for_test_enclosing_runtime(db.clone());
        db.shutdown().await;
        let auth = spacetimedb_lib::identity::AuthCtx::new(db.owner_identity(), db.owner_identity());
        for statement in ["SELECT * FROM st_client", "SET env.LATE = 'denied'"] {
            let error = crate::sql::execute::run(
                db.clone(),
                statement.into(),
                auth.clone(),
                Some(subscriptions.clone()),
                None,
                &mut vec![],
            )
            .await
            .err()
            .unwrap();
            assert!(matches!(error, DBError::DatabaseClosed), "{error}");
        }
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::Unsubscribe);
        environment::set(&db, &mut tx, "LATE", "denied").unwrap();
        let event = ModuleEvent {
            timestamp: spacetimedb_lib::Timestamp::now(),
            caller_identity: db.owner_identity(),
            caller_connection_id: None,
            function_call: ModuleFunctionCall::update(),
            status: EventStatus::Committed(DatabaseUpdate::default()),
            reducer_return_value: None,
            energy_quanta_used: crate::energy::EnergyQuanta::ZERO,
            host_execution_duration: std::time::Duration::ZERO,
            request_id: None,
            timer: None,
        };
        let error = subscriptions.commit_and_broadcast_event(None, event, tx).err().unwrap();
        assert!(matches!(error, DBError::DatabaseClosed));
        let tx = db.begin_tx(Workload::Internal);
        assert_eq!(environment::get(&tx, "LATE").unwrap(), None);
        let _ = db.release_tx(tx);
    });
}
