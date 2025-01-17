use super::execution_unit::QueryHash;
use super::module_subscription_manager::{Plan, SubscriptionManager};
use super::query::compile_read_only_query;
use super::tx::DeltaTx;
use crate::client::messages::{
    SubscriptionError, SubscriptionMessage, SubscriptionResult, SubscriptionRows, SubscriptionUpdateMessage,
    TransactionUpdateMessage,
};
use crate::client::{ClientActorId, ClientConnectionSender, Protocol};
use crate::db::datastore::locking_tx_datastore::tx::TxId;
use crate::db::relational_db::{MutTx, RelationalDB, Tx};
use crate::error::DBError;
use crate::estimation::estimate_rows_scanned;
use crate::execution_context::Workload;
use crate::host::module_host::{DatabaseUpdate, EventStatus, ModuleEvent};
use crate::messages::websocket::Subscribe;
use crate::vm::check_row_limit;
use crate::worker_metrics::WORKER_METRICS;
use parking_lot::RwLock;
use spacetimedb_client_api_messages::websocket::{
    BsatnFormat, FormatSwitch, JsonFormat, SubscribeSingle, TableUpdate, Unsubscribe,
};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::Identity;
use spacetimedb_query::{execute_plans, SubscribePlan};
use std::{sync::Arc, time::Instant};

type Subscriptions = Arc<RwLock<SubscriptionManager>>;

#[derive(Debug, Clone)]
pub struct ModuleSubscriptions {
    relational_db: Arc<RelationalDB>,
    /// If taking a lock (tx) on the db at the same time, ALWAYS lock the db first.
    /// You will deadlock otherwise.
    subscriptions: Subscriptions,
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

    /// Run auth and row limit checks for a new subscriber, then compute the initial query results.
    fn evaluate_initial_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        query: Arc<Plan>,
        tx: &TxId,
        auth: &AuthCtx,
    ) -> Result<FormatSwitch<TableUpdate<BsatnFormat>, TableUpdate<JsonFormat>>, DBError> {
        let comp = sender.config.compression;
        let plan = SubscribePlan::from_delta_plan(&query);

        check_row_limit(
            &plan,
            &self.relational_db,
            tx,
            |plan, tx| estimate_rows_scanned(tx, plan),
            auth,
        )?;

        let tx = DeltaTx::from(tx);
        Ok(match sender.config.protocol {
            Protocol::Binary => FormatSwitch::Bsatn(plan.collect_table_update(comp, &tx)?),
            Protocol::Text => FormatSwitch::Json(plan.collect_table_update(comp, &tx)?),
        })
    }

    #[tracing::instrument(skip_all)]
    pub fn add_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        request: SubscribeSingle,
        timer: Instant,
        _assert: Option<AssertTxFn>,
    ) -> Result<(), DBError> {
        let tx = scopeguard::guard(self.relational_db.begin_tx(Workload::Subscribe), |tx| {
            self.relational_db.release_tx(tx);
        });
        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let guard = self.subscriptions.read();
        let query = super::query::WHITESPACE.replace_all(&request.query, " ");
        let sql = query.trim();
        let hash = QueryHash::from_string(sql);
        let query = if let Some(unit) = guard.query(&hash) {
            unit
        } else {
            let compiled = compile_read_only_query(&auth, &tx, sql)?;
            Arc::new(compiled)
        };

        drop(guard);

        let table_rows = self.evaluate_initial_subscription(sender.clone(), query.clone(), &tx, &auth)?;

        // It acquires the subscription lock after `eval`, allowing `add_subscription` to run concurrently.
        // This also makes it possible for `broadcast_event` to get scheduled before the subsequent part here
        // but that should not pose an issue.
        let mut subscriptions = self.subscriptions.write();
        subscriptions.add_subscription(sender.clone(), query.clone(), request.query_id)?;

        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.relational_db.database_identity())
            .set(subscriptions.num_unique_queries() as i64);

        #[cfg(test)]
        if let Some(assert) = _assert {
            assert(&tx);
        }

        // NOTE: It is important to send the state in this thread because if you spawn a new
        // thread it's possible for messages to get sent to the client out of order. If you do
        // spawn in another thread messages will need to be buffered until the state is sent out
        // on the wire
        let _ = sender.send_message(SubscriptionMessage {
            request_id: Some(request.request_id),
            query_id: Some(request.query_id),
            timer: Some(timer),
            result: SubscriptionResult::Subscribe(SubscriptionRows {
                table_id: query.table_id(),
                table_name: query.table_name(),
                table_rows,
            }),
        });
        Ok(())
    }

    pub fn remove_subscription(
        &self,
        sender: Arc<ClientConnectionSender>,
        request: Unsubscribe,
        timer: Instant,
    ) -> Result<(), DBError> {
        let mut subscriptions = self.subscriptions.write();
        let query = match subscriptions.remove_subscription((sender.id.identity, sender.id.address), request.query_id) {
            Ok(query) => query,
            Err(error) => {
                // Apparently we ignore errors sending messages.
                let _ = sender.send_message(SubscriptionMessage {
                    request_id: Some(request.request_id),
                    query_id: None,
                    timer: Some(timer),
                    result: SubscriptionResult::Error(SubscriptionError {
                        table_id: None,
                        message: error.to_string().into(),
                    }),
                });
                return Ok(());
            }
        };

        let tx = scopeguard::guard(self.relational_db.begin_tx(Workload::Unsubscribe), |tx| {
            self.relational_db.release_tx(tx);
        });
        let auth = AuthCtx::new(self.owner_identity, sender.id.identity);
        let table_rows = self.evaluate_initial_subscription(sender.clone(), query.clone(), &tx, &auth)?;

        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.relational_db.database_identity())
            .set(subscriptions.num_unique_queries() as i64);
        let _ = sender.send_message(SubscriptionMessage {
            request_id: Some(request.request_id),
            query_id: Some(request.query_id),
            timer: Some(timer),
            result: SubscriptionResult::Unsubscribe(SubscriptionRows {
                table_id: query.table_id(),
                table_name: query.table_name(),
                table_rows,
            }),
        });
        Ok(())
    }

    /// Add a subscriber to the module. NOTE: this function is blocking.
    /// This is used for the legacy subscription API which uses a set of queries.
    #[tracing::instrument(skip_all)]
    pub fn add_legacy_subscriber(
        &self,
        sender: Arc<ClientConnectionSender>,
        subscription: Subscribe,
        timer: Instant,
        _assert: Option<AssertTxFn>,
    ) -> Result<(), DBError> {
        let tx = scopeguard::guard(self.relational_db.begin_tx(Workload::Subscribe), |tx| {
            self.relational_db.release_tx(tx);
        });
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
                        .map(Arc::new),
                );
                continue;
            }
            let hash = QueryHash::from_string(sql);
            if let Some(unit) = guard.query(&hash) {
                queries.push(unit);
            } else {
                let compiled = compile_read_only_query(&auth, &tx, sql)?;
                queries.push(Arc::new(compiled));
            }
        }

        drop(guard);

        let comp = sender.config.compression;
        let plans = queries
            .iter()
            .map(|plan| &***plan)
            .map(SubscribePlan::from_delta_plan)
            .collect::<Vec<_>>();

        fn rows_scanned(tx: &TxId, plans: &[SubscribePlan]) -> u64 {
            plans
                .iter()
                .map(|plan| estimate_rows_scanned(tx, plan))
                .fold(0, |acc, n| acc.saturating_add(n))
        }

        check_row_limit(
            &plans,
            &self.relational_db,
            &tx,
            |plan, tx| rows_scanned(tx, plan),
            &auth,
        )?;

        let tx = DeltaTx::from(&*tx);
        let database_update = match sender.config.protocol {
            Protocol::Text => FormatSwitch::Json(execute_plans(plans, comp, &tx)?),
            Protocol::Binary => FormatSwitch::Bsatn(execute_plans(plans, comp, &tx)?),
        };

        // It acquires the subscription lock after `eval`, allowing `add_subscription` to run concurrently.
        // This also makes it possible for `broadcast_event` to get scheduled before the subsequent part here
        // but that should not pose an issue.
        let mut subscriptions = self.subscriptions.write();
        subscriptions.set_legacy_subscription(sender.clone(), queries.into_iter());
        let num_queries = subscriptions.num_unique_queries();

        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.relational_db.database_identity())
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
            database_update,
            request_id: Some(request_id),
            timer: Some(timer),
        });
        Ok(())
    }

    pub fn remove_subscriber(&self, client_id: ClientActorId) {
        let mut subscriptions = self.subscriptions.write();
        subscriptions.remove_all_subscriptions(&(client_id.identity, client_id.address));
        WORKER_METRICS
            .subscription_queries
            .with_label_values(&self.relational_db.database_identity())
            .set(subscriptions.num_unique_queries() as i64);
    }

    /// Commit a transaction and broadcast its ModuleEvent to all interested subscribers.
    pub fn commit_and_broadcast_event(
        &self,
        caller: Option<&ClientConnectionSender>,
        mut event: ModuleEvent,
        tx: MutTx,
    ) -> Result<Result<Arc<ModuleEvent>, WriteConflict>, DBError> {
        // Take a read lock on `subscriptions` before committing tx
        // else it can result in subscriber receiving duplicate updates.
        let subscriptions = self.subscriptions.read();
        let stdb = &self.relational_db;
        // Downgrade mutable tx.
        // Ensure tx is released/cleaned up once out of scope.
        let (read_tx, tx_data) = match &mut event.status {
            EventStatus::Committed(db_update) => {
                let Some((tx_data, read_tx)) = stdb.commit_tx_downgrade(tx, Workload::Update)? else {
                    return Ok(Err(WriteConflict));
                };
                *db_update = DatabaseUpdate::from_writes(&tx_data);
                (read_tx, Some(tx_data))
            }
            EventStatus::Failed(_) | EventStatus::OutOfEnergy => {
                (stdb.rollback_mut_tx_downgrade(tx, Workload::Update), None)
            }
        };

        let read_tx = scopeguard::guard(read_tx, |tx| {
            self.relational_db.release_tx(tx);
        });

        let read_tx = tx_data
            .as_ref()
            .map(|tx_data| DeltaTx::new(&read_tx, tx_data))
            .unwrap_or_else(|| DeltaTx::from(&*read_tx));

        let event = Arc::new(event);

        match &event.status {
            EventStatus::Committed(_) => subscriptions.eval_updates(&read_tx, event.clone(), caller),
            EventStatus::Failed(_) => {
                if let Some(client) = caller {
                    let message = TransactionUpdateMessage {
                        event: Some(event.clone()),
                        database_update: SubscriptionUpdateMessage::default_for_protocol(client.config.protocol, None),
                    };
                    let _ = client.send_message(message);
                } else {
                    log::trace!("Reducer failed but there is no client to send the failure to!")
                }
            }
            EventStatus::OutOfEnergy => {} // ?
        }

        Ok(Ok(event))
    }
}

pub struct WriteConflict;

#[cfg(test)]
mod tests {
    use super::{AssertTxFn, ModuleSubscriptions};
    use crate::client::{ClientActorId, ClientConfig, ClientConnectionSender};
    use crate::db::relational_db::tests_utils::{insert, TestDB};
    use crate::db::relational_db::RelationalDB;
    use crate::error::DBError;
    use crate::execution_context::Workload;
    use spacetimedb_client_api_messages::websocket::Subscribe;
    use spacetimedb_lib::db::auth::StAccess;
    use spacetimedb_lib::{error::ResultTest, AlgebraicType, Identity};
    use spacetimedb_sats::product;
    use std::time::Instant;
    use std::{sync::Arc, time::Duration};
    use tokio::sync::mpsc;

    fn add_subscriber(db: Arc<RelationalDB>, sql: &str, assert: Option<AssertTxFn>) -> Result<(), DBError> {
        let owner = Identity::from_byte_array([1; 32]);
        let client = ClientActorId::for_test(Identity::ZERO);
        let config = ClientConfig::for_test();
        let sender = Arc::new(ClientConnectionSender::dummy(client, config));
        let module_subscriptions = ModuleSubscriptions::new(db.clone(), owner);

        let subscribe = Subscribe {
            query_strings: [sql.into()].into(),
            request_id: 0,
        };
        module_subscriptions.add_legacy_subscriber(sender, subscribe, Instant::now(), assert)
    }

    /// Asserts that a subscription holds a tx handle for the entire length of its evaluation.
    #[test]
    fn test_tx_subscription_ordering() -> ResultTest<()> {
        let test_db = TestDB::durable()?;

        let runtime = test_db.runtime().cloned().unwrap();
        let db = Arc::new(test_db.db.clone());

        // Create table with one row
        let table_id = db.create_table_for_test("T", &[("a", AlgebraicType::U8)], &[])?;
        db.with_auto_commit(Workload::ForTests, |tx| {
            insert(&db, tx, table_id, &product!(1_u8)).map(drop)
        })?;

        let (send, mut recv) = mpsc::unbounded_channel();

        // Subscribing to T should return a single row.
        let db2 = db.clone();
        let query_handle = runtime.spawn_blocking(move || {
            add_subscriber(
                db.clone(),
                "select * from T",
                Some(Arc::new(move |tx: &_| {
                    // Wake up writer thread after starting the reader tx
                    let _ = send.send(());
                    // Then go to sleep
                    std::thread::sleep(Duration::from_secs(1));
                    // Assuming subscription evaluation holds a lock on the db,
                    // any mutations to T will necessarily occur after,
                    // and therefore we should only see a single row returned.
                    assert_eq!(1, db.iter(tx, table_id).unwrap().count());
                })),
            )
        });

        // Write a second row to T concurrently with the reader thread
        let write_handle = runtime.spawn(async move {
            let _ = recv.recv().await;
            db2.with_auto_commit(Workload::ForTests, |tx| {
                insert(&db2, tx, table_id, &product!(2_u8)).map(drop)
            })
        });

        runtime.block_on(write_handle)??;
        runtime.block_on(query_handle)??;

        test_db.close()?;

        Ok(())
    }

    #[test]
    fn subs_cannot_access_private_tables() -> ResultTest<()> {
        let test_db = TestDB::durable()?;
        let db = Arc::new(test_db.db.clone());

        // Create a public table.
        let indexes = &[0.into()];
        let cols = &[("a", AlgebraicType::U8)];
        let _ = db.create_table_for_test("public", cols, indexes)?;

        // Create a private table.
        let _ = db.create_table_for_test_with_access("private", cols, indexes, StAccess::Private)?;

        // We can subscribe to a public table.
        let subscribe = |sql| add_subscriber(db.clone(), sql, None);
        assert!(subscribe("SELECT * FROM public").is_ok());

        // We cannot subscribe when a private table is mentioned,
        // not even when in a join where the projection doesn't mention the table,
        // as the mere fact of joining can leak information from the private table.
        for sql in [
            "SELECT * FROM private",
            // Even if the query will return no rows, we still reject it.
            "SELECT * FROM private WHERE false",
            "SELECT private.* FROM private",
            "SELECT public.* FROM public JOIN private ON public.a = private.a WHERE private.a = 1",
            "SELECT private.* FROM private JOIN public ON private.a = public.a WHERE public.a = 1",
        ] {
            assert!(subscribe(sql).is_err(),);
        }

        Ok(())
    }
}
