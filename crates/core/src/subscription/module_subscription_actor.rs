use super::execution_unit::{ExecutionUnit, QueryHash};
use super::module_subscription_manager::SubscriptionManager;
use super::query::compile_read_only_query;
use super::subscription::ExecutionSet;
use crate::client::messages::{SubscriptionUpdate, SubscriptionUpdateMessage, TransactionUpdateMessage};
use crate::client::{ClientActorId, ClientConnectionSender};
use crate::db::relational_db::{RelationalDB, Tx};
use crate::error::{DBError, SubscriptionError};
use crate::execution_context::ExecutionContext;
use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent};
use crate::protobuf::client_api::Subscribe;
use crate::worker_metrics::WORKER_METRICS;
use parking_lot::RwLock;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Identity;
use std::{sync::Arc, time::Instant};

type Subscriptions = Arc<RwLock<SubscriptionManager>>;

#[derive(Debug)]
pub struct ModuleSubscriptions {
    relational_db: Arc<RelationalDB>,
    pub subscriptions: Subscriptions,
    owner_identity: Identity,
}

type AssertTxFn = Arc<dyn Fn(&Tx)>;

impl ModuleSubscriptions {
    pub fn new(relational_db: Arc<RelationalDB>, owner_identity: Identity) -> Self {
        Self {
            relational_db,
            subscriptions: Arc::new(RwLock::new(SubscriptionManager::default())),
            owner_identity,
        }
    }

    /// Add a subscriber to the module. NOTE: this function is blocking.
    #[tracing::instrument(skip_all)]
    pub fn add_subscriber(
        &self,
        sender: Arc<ClientConnectionSender>,
        subscription: Subscribe,
        timer: Instant,
        _assert: Option<AssertTxFn>,
    ) -> Result<(), DBError> {
        let ctx = ExecutionContext::subscribe(
            self.relational_db.address(),
            self.relational_db.read_config().slow_query,
        );
        let tx = scopeguard::guard(self.relational_db.begin_tx(), |tx| {
            self.relational_db.release_tx(&ctx, tx);
        });
        // check for backward comp.
        let request_id = subscription.request_id;
        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let mut queries = vec![];

        let guard = self.subscriptions.read();

        for sql in subscription
            .query_strings
            .iter()
            .map(|sql| super::query::WHITESPACE.replace_all(sql, " "))
        {
            let sql = sql.trim();
            if sql == super::query::SUBSCRIBE_TO_ALL_QUERY {
                queries.extend(
                    super::subscription::get_all(&self.relational_db, &tx, &auth)?
                        .into_iter()
                        .map(|query| {
                            let hash = QueryHash::from_string(&query.sql);
                            ExecutionUnit::new(query, hash).map(Arc::new)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                );
                continue;
            }
            let hash = QueryHash::from_string(sql);
            if let Some(unit) = guard.query(&hash) {
                queries.push(unit);
            } else {
                let mut compiled = compile_read_only_query(&self.relational_db, &tx, sql)?;
                if compiled.len() > 1 {
                    return Result::Err(
                        SubscriptionError::Unsupported(String::from("Multiple statements in subscription query"))
                            .into(),
                    );
                }
                queries.push(Arc::new(ExecutionUnit::new(compiled.remove(0), hash)?));
            }
        }

        drop(guard);

        let execution_set: ExecutionSet = queries.into();
        let database_update = execution_set.eval(&ctx, sender.protocol, &self.relational_db, &tx)?;

        WORKER_METRICS
            .initial_subscription_evals
            .with_label_values(&self.relational_db.address())
            .inc();

        // It acquires the subscription lock after `eval`, allowing `add_subscription` to run concurrently.
        // This also makes it possible for `broadcast_event` to get scheduled before the subsequent part here
        // but that should not pose an issue.
        let mut subscriptions = self.subscriptions.write();
        subscriptions.remove_subscription(&(sender.id.identity, sender.id.address));
        subscriptions.add_subscription(sender.clone(), execution_set.into_iter());
        let num_queries = subscriptions.num_queries();

        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.relational_db.address())
            .set(num_queries as i64);

        #[cfg(test)]
        if let Some(assert) = _assert {
            assert(&tx);
        }

        // NOTE: It is important to send the state in this thread because if you spawn a new
        // thread it's possible for messages to get sent to the client out of order. If you do
        // spawn in another thread messages will need to be buffered until the state is sent out
        // on the wire
        let _ = sender.send_message(SubscriptionUpdateMessage {
            subscription_update: SubscriptionUpdate {
                database_update,
                request_id: Some(request_id),
                timer: Some(timer),
            },
        });
        Ok(())
    }

    pub fn remove_subscriber(&self, client_id: ClientActorId) {
        let mut subscriptions = self.subscriptions.write();
        subscriptions.remove_subscription(&(client_id.identity, client_id.address));
        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.relational_db.address())
            .set(subscriptions.num_queries() as i64);
    }

    /// Broadcast a ModuleEvent to all interested subscribers.
    ///
    /// It's recommended to take a read lock on `subscriptions` field *before* you commit
    /// the transaction that will give you the event you pass here, to prevent a race condition
    /// where a just-added subscriber receives the same update twice.
    pub async fn broadcast_event(
        &self,
        client: Option<&ClientConnectionSender>,
        subscriptions: &SubscriptionManager,
        event: Arc<ModuleEvent>,
    ) {
        match event.status {
            EventStatus::Committed(_) => {
                tokio::task::block_in_place(|| self.broadcast_commit_event(subscriptions, event))
            }
            EventStatus::Failed(_) => {
                if let Some(client) = client {
                    let message = TransactionUpdateMessage::<DatabaseUpdate> {
                        event,
                        database_update: <_>::default(),
                    };
                    let _ = client.send_message(message);
                } else {
                    log::trace!("Reducer failed but there is no client to send the failure to!")
                }
            }
            EventStatus::OutOfEnergy => {} // ?
        }
    }

    /// A blocking version of [`broadcast_event`][Self::broadcast_event].
    pub fn blocking_broadcast_event(
        &self,
        client: Option<&ClientConnectionSender>,
        subscriptions: &SubscriptionManager,
        event: Arc<ModuleEvent>,
    ) {
        tokio::runtime::Handle::current().block_on(self.broadcast_event(client, subscriptions, event))
    }

    /// Broadcast the commit event to all interested subscribers.
    ///
    /// This function is blocking, even though it returns a future. The returned future resolves
    /// once all updates have been successfully added to the subscribers' send queues (i.e. after
    /// it resolves, it's guaranteed that if you call `subscriber.send(x)` the client will receive
    /// x after they receive this subscription update).
    fn broadcast_commit_event(&self, subscriptions: &SubscriptionManager, event: Arc<ModuleEvent>) {
        subscriptions.eval_updates(&self.relational_db, event)
    }
}

#[cfg(test)]
mod tests {
    use super::ModuleSubscriptions;
    use crate::client::messages::{SerializableMessage, SubscriptionUpdate, TransactionUpdateMessage};
    use crate::client::{ClientActorId, ClientConnectionSender, Protocol};
    use crate::db::relational_db::tests_utils::TestDB;
    use crate::energy::EnergyQuanta;
    use crate::execution_context::ExecutionContext;
    use crate::host::module_host::{
        DatabaseTableUpdate, DatabaseUpdate, EventStatus, ModuleEvent, ModuleFunctionCall, ProtocolDatabaseUpdate,
    };
    use crate::host::{ArgsTuple, Timestamp};
    use spacetimedb_client_api_messages::client_api::Subscribe;
    use spacetimedb_lib::{error::ResultTest, AlgebraicType, Identity};
    use spacetimedb_sats::product;
    use std::time::Instant;
    use std::{sync::Arc, time::Duration};
    use tokio::sync::mpsc;

    #[test]
    /// Asserts that a subscription holds a tx handle for the entire length of its evaluation.
    fn test_tx_subscription_ordering() -> ResultTest<()> {
        let test_db = TestDB::durable()?;

        let runtime = test_db.runtime().cloned().unwrap();
        let db = Arc::new(test_db.db.clone());

        // Create table with one row
        let table_id = db.create_table_for_test("T", &[("a", AlgebraicType::U8)], &[])?;
        db.with_auto_commit(&ExecutionContext::default(), |tx| {
            db.insert(tx, table_id, product!(1_u8))
        })?;

        let id = Identity::ZERO;
        let client_id = ClientActorId::for_test(None, None);
        let sender = Arc::new(ClientConnectionSender::dummy(client_id, Protocol::Binary));
        let module_subscriptions = ModuleSubscriptions::new(db.clone(), id);

        let (send, mut recv) = mpsc::unbounded_channel();

        // Subscribing to T should return a single row
        let query_handle = runtime.spawn_blocking(move || {
            let db = module_subscriptions.relational_db.clone();
            let query_strings = vec!["select * from T".into()];
            module_subscriptions.add_subscriber(
                sender,
                Subscribe {
                    query_strings,
                    request_id: 0,
                },
                Instant::now(),
                Some(Arc::new(move |tx: &_| {
                    // Wake up writer thread after starting the reader tx
                    let _ = send.send(());
                    // Then go to sleep
                    std::thread::sleep(Duration::from_secs(1));
                    let ctx = ExecutionContext::default();
                    // Assuming subscription evaluation holds a lock on the db,
                    // any mutations to T will necessarily occur after,
                    // and therefore we should only see a single row returned.
                    assert_eq!(1, db.iter(&ctx, tx, table_id).unwrap().count());
                })),
            )
        });

        // Write a second row to T concurrently with the reader thread
        let write_handle = runtime.spawn(async move {
            let _ = recv.recv().await;
            db.with_auto_commit(&ExecutionContext::default(), |tx| {
                db.insert(tx, table_id, product!(2_u8))
            })
        });

        runtime.block_on(write_handle)??;
        runtime.block_on(query_handle)??;

        test_db.close()?;

        Ok(())
    }

    #[test]
    /// checks if multiple clients with the same identity are properly handled
    fn test_subscriptions_for_the_same_client_identity() -> ResultTest<()> {
        let test_db = TestDB::durable()?;
        let runtime = test_db.runtime().cloned().unwrap();

        // Create table with no rows
        let db = Arc::new(test_db.db.clone());
        let table_id = db.create_table_for_test("T", &[("a", AlgebraicType::U8)], &[])?;

        let id = ClientActorId::for_test(None, None);
        let sender = Arc::new(ClientConnectionSender::dummy(id, Protocol::Binary));
        let module_subscriptions = ModuleSubscriptions::new(db.clone(), id.identity);

        let client_id0 = ClientActorId::for_test(None, None);
        let client_id1 = ClientActorId::for_test(Some(client_id0.identity), None);
        let (client0, mut rx0) = ClientConnectionSender::dummy_with_channel(client_id0, Protocol::Binary);
        let (client1, mut rx1) = ClientConnectionSender::dummy_with_channel(client_id1, Protocol::Binary);

        // Subscribing to T should return a single row
        let query_strings = vec!["select * from T where a = 1".into()];
        module_subscriptions
            .add_subscriber(
                Arc::new(client0),
                Subscribe {
                    query_strings,
                    request_id: 0,
                },
                Instant::now(),
                None,
            )
            .unwrap();

        let query_strings = vec!["select * from T where a = 2".into()];
        module_subscriptions
            .add_subscriber(
                Arc::new(client1),
                Subscribe {
                    query_strings,
                    request_id: 1,
                },
                Instant::now(),
                None,
            )
            .unwrap();

        let inserts = Arc::new([product!(1u8), product!(2u8), product!(2u8)]);
        let table_update = DatabaseTableUpdate {
            table_id,
            table_name: Box::from("T"),
            inserts,
            deletes: Arc::new([]),
        };
        let database_update = DatabaseUpdate {
            tables: vec![table_update],
        };
        let event = Arc::new(ModuleEvent {
            timestamp: Timestamp::now(),
            caller_identity: client_id0.identity,
            caller_address: None,
            function_call: ModuleFunctionCall {
                reducer: "DummyReducer".into(),
                args: ArgsTuple::nullary(),
            },
            status: EventStatus::Committed(database_update),
            energy_quanta_used: EnergyQuanta::ZERO,
            host_execution_duration: Duration::default(),
            request_id: None,
            timer: None,
        });

        runtime.block_on(async move {
            tokio::task::block_in_place(move || {
                let subscriptions = module_subscriptions.subscriptions.read();
                module_subscriptions.blocking_broadcast_event(Some(&sender), &subscriptions, event);
            });
            tokio::time::sleep(Duration::from_secs(4)).await;
            tokio::time::timeout(Duration::from_millis(100), async move {
                rx0.recv().await.expect("Expected subscription update message");
                let m0 = rx0.recv().await.expect("Expected transaction update message");
                rx1.recv().await.expect("Expected subscription update message");
                let m1 = rx1.recv().await.expect("Expected transaction update message");

                // check if the first client got the update with only 1 row and the second client
                // got the update with 2 rows
                assert!(matches!(m0,
                    SerializableMessage::ProtocolUpdate(
                        TransactionUpdateMessage {
                            database_update: SubscriptionUpdate {
                                database_update: ProtocolDatabaseUpdate { tables, .. },
                            ..},
                        ..}) if tables.clone().left().unwrap()[0].table_row_operations.len() == 1));
                assert!(matches!(m1,
                    SerializableMessage::ProtocolUpdate(
                        TransactionUpdateMessage {
                            database_update: SubscriptionUpdate {
                                database_update: ProtocolDatabaseUpdate { tables, .. },
                            ..},
                        ..}) if tables.clone().left().unwrap()[0].table_row_operations.len() == 2));
            })
            .await
            .unwrap();
        });

        Ok(())
    }
}
