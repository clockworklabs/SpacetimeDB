use crate::db::{environment, relational_db::tests_utils::TestDB};
use crate::error::DBError;
use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent, ModuleFunctionCall};
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use spacetimedb_datastore::{execution_context::Workload, traits::IsolationLevel};

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
            execution_budget_used: spacetimedb_client_api_messages::energy::FunctionBudget::ZERO,
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
